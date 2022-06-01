mod activity;
mod decoder;
mod log;
mod media_format;
mod network;
mod util;

mod debug;

use activity::{NativeActivity, NativeActivityEvent, NativeActivityInstance};
use ndk_sys::{AInputQueue, ANativeActivity, ANativeWindow};
use std::{
    ptr::NonNull,
    sync::{Arc, Barrier},
    thread,
};

// adb logcat -v raw -s client-android
// adb install target\debug\apk\client-android.apk
// C:\Users\Rafael\AppData\Local\Android\Sdk\emulator\emulator -avd Pixel_3_API_31

#[no_mangle]
unsafe extern "C" fn ANativeActivity_onCreate(
    activity: *mut ANativeActivity,
    _saved_state: *mut u8,
    _saved_state_size: usize,
) {
    spawn_main_loop(activity);
}

fn spawn_main_loop(activity: *mut ANativeActivity) {
    // SAFETY: Assuming the `activity` is valid
    let activity = unsafe { NativeActivity::from_ptr(activity) };
    if !activity.instance.is_null() {
        // SAFETY: The pointer is assumed to be previously allocated `NativeActivityInstance`
        unsafe { NativeActivityInstance::drop_ptr(activity.instance) };
    }

    let (sender, receiver) = crossbeam_channel::unbounded::<NativeActivityEvent>();
    let sender_barrier = Arc::new(Barrier::new(2));
    let receiver_barrier = sender_barrier.clone();

    let instance = Box::into_raw(Box::new(NativeActivityInstance::new(
        sender,
        sender_barrier,
    )));

    activity.instance = instance.cast();
    activity.set_callbacks();

    thread::spawn(
        move || match main_loop(activity, receiver, receiver_barrier) {
            Ok(_) => (),
            Err(e) => error!("Error in main loop: {}", e),
        },
    );
}

fn main_loop(
    activity: &mut NativeActivity,
    receiver: crossbeam_channel::Receiver<NativeActivityEvent>,
    barrier: Arc<Barrier>,
) -> anyhow::Result<()> {
    fn wait_for_input_queue_and_native_window(
        receiver: &crossbeam_channel::Receiver<NativeActivityEvent>,
    ) -> Option<(NonNull<AInputQueue>, NonNull<ANativeWindow>)> {
        let mut input_queue = None;
        let mut native_window = None;
        while input_queue.is_none() || native_window.is_none() {
            match receiver.recv() {
                Ok(msg) => match msg {
                    NativeActivityEvent::InputQueueCreated(q) => {
                        input_queue = Some(NonNull::new(q)?);
                    }
                    NativeActivityEvent::NativeWindowCreated(w) => {
                        native_window = Some(NonNull::new(w)?);
                    }
                    NativeActivityEvent::Destroy => return None,
                    _ => (),
                },
                Err(_) => return None,
            }
        }
        Some((input_queue?, native_window?))
    }

    let (_input_queue, mut native_window) = wait_for_input_queue_and_native_window(&receiver)
        .ok_or_else(|| anyhow::anyhow!("Unable to receive `ANativeWindow`"))?;

    let asset_manager = activity.assetManager;
    let csd = debug::get_csd(asset_manager)?;
    let packets = debug::get_h264_packets(asset_manager)?;

    let format = media_format::MediaFormat::create_video_format(
        media_format::VideoType::H264,
        1920,
        1080,
        60,
        &csd,
    )?;

    let decoder = decoder::MediaDecoder::create_from_format(&format, native_window)?;

    let mut time = 0;
    let mut packet_index = 0;

    loop {
        match receiver.try_recv() {
            Ok(msg) => {
                info!("{:?}", &msg);
                match msg {
                    // NativeActivityEvent::Start => todo!(),
                    // NativeActivityEvent::Resume => todo!(),
                    // NativeActivityEvent::SaveInstanceState(_) => todo!(),
                    // NativeActivityEvent::Pause => todo!(),
                    // NativeActivityEvent::Stop => exit_loop = true,
                    NativeActivityEvent::Destroy => break,
                    // NativeActivityEvent::WindowFocusChanged(_) => todo!(),
                    NativeActivityEvent::NativeWindowCreated(w) => {
                        if w != native_window.as_ptr() {
                            native_window = NonNull::new(w).unwrap();
                            decoder.set_output_surface(native_window)?;
                        }
                    }
                    NativeActivityEvent::NativeWindowResized(w) => {
                        if w != native_window.as_ptr() {
                            native_window = NonNull::new(w).unwrap();
                            decoder.set_output_surface(native_window)?;
                        }
                    }
                    // NativeActivityEvent::NativeWindowRedrawNeeded(_) => todo!(),
                    NativeActivityEvent::NativeWindowDestroyed(_) => break,
                    // NativeActivityEvent::InputQueueCreated(_) => todo!(),
                    // NativeActivityEvent::InputQueueDestroyed(_) => todo!(),
                    // NativeActivityEvent::ContentRectChanged(_) => todo!(),
                    // NativeActivityEvent::ConfigurationChanged => todo!(),
                    // NativeActivityEvent::LowMemory => todo!(),
                    _ => (),
                }
            }
            Err(e) => match e {
                crossbeam_channel::TryRecvError::Empty => {
                    if let Some(packet) = packets.get(packet_index) {
                        let end_of_stream = if packet_index == packets.len() - 1 {
                            true
                        } else {
                            false
                        };
                        if decoder.try_decode(&packet, time, end_of_stream)? {
                            time += 16_666;
                            packet_index += 1;
                        }
                    }
                    decoder.try_render()?;
                }
                crossbeam_channel::TryRecvError::Disconnected => (),
            },
        };
    }
    info!("Exiting");
    let _ignored_result = barrier.wait();
    Ok(()) // Normal loop exit
}
