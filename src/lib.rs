mod decoder;
mod log;
mod media_format;

mod debug;

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use ndk_sys::{AInputQueue, ANativeActivity, ANativeWindow, ARect};
use std::{
    os::raw::{c_int, c_ulong, c_void},
    ptr::NonNull,
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
    let (sender, receiver) = crossbeam_channel::bounded::<NativeActivityEvent>(2);
    let sender = Box::into_raw(Box::new(sender));

    {
        let activity = &mut *activity;
        activity.instance = sender.cast();

        let callbacks = &mut *activity.callbacks;
        callbacks.onStart = Some(on_start);
        callbacks.onResume = Some(on_resume);
        callbacks.onSaveInstanceState = Some(on_save_instance_state);
        callbacks.onPause = Some(on_pause);
        callbacks.onStop = Some(on_stop);
        callbacks.onDestroy = Some(on_destroy);
        callbacks.onWindowFocusChanged = Some(on_window_focus_changed);
        callbacks.onNativeWindowCreated = Some(on_native_window_created);
        callbacks.onNativeWindowResized = Some(on_native_window_resized);
        callbacks.onNativeWindowRedrawNeeded = Some(on_native_window_redraw_needed);
        callbacks.onNativeWindowDestroyed = Some(on_native_window_destroyed);
        callbacks.onInputQueueCreated = Some(on_input_queue_created);
        callbacks.onInputQueueDestroyed = Some(on_input_queue_destroyed);
        callbacks.onContentRectChanged = Some(on_content_rect_changed);
        callbacks.onConfigurationChanged = Some(on_configuration_changed);
        callbacks.onLowMemory = Some(on_low_memory);
    }

    thread::spawn(move || match main_loop(receiver) {
        Ok(_) => (),
        Err(e) => error!("{}", e),
    });
}

fn wait_for_window(
    receiver: &Receiver<NativeActivityEvent>,
) -> Option<(NonNull<ANativeActivity>, NonNull<ANativeWindow>)> {
    let mut activity = None;
    while let Ok(msg) = receiver.recv() {
        match msg {
            NativeActivityEvent::Start(a) => activity = NonNull::new(a),
            NativeActivityEvent::NativeWindowCreated(w) => {
                if let (Some(activity), Some(window)) = (activity, NonNull::new(w)) {
                    return Some((activity, window));
                } else {
                    return None;
                }
            }
            NativeActivityEvent::Destroy => return None,
            _ => (),
        }
    }
    None
}

fn main_loop(receiver: Receiver<NativeActivityEvent>) -> anyhow::Result<()> {
    let (activity, window) = match wait_for_window(&receiver) {
        Some(val) => val,
        None => anyhow::bail!("Unable to receive `ANativeWindow`"),
    };

    let mut destroy_called = false;


    let asset_manager = unsafe { (&*activity.as_ptr()).assetManager };
    let csd = debug::get_csd(asset_manager)?;
    let packets = debug::get_h264_packets(asset_manager)?;
    let format = media_format::MediaFormat::create_video_format(
        media_format::VideoType::H264,
        1920,
        1080,
        60,
        &csd,
    )?;

    let decoder = decoder::MediaDecoder::create_from_format(&format, window)?;
    
    info!("GREAT SUCCESS");

    loop {
        let msg = match receiver.try_recv() {
            Ok(msg) => match msg {
                // NativeActivityEvent::Start => todo!(),
                // NativeActivityEvent::Resume => todo!(),
                // NativeActivityEvent::SaveInstanceState(_) => todo!(),
                // NativeActivityEvent::Pause => todo!(),
                // NativeActivityEvent::Stop => todo!(),
                NativeActivityEvent::Destroy => destroy_called = true,
                // NativeActivityEvent::WindowFocusChanged(_) => todo!(),
                // NativeActivityEvent::NativeWindowCreated(_) => todo!(),
                // NativeActivityEvent::NativeWindowResized(_) => todo!(),
                // NativeActivityEvent::NativeWindowRedrawNeeded(_) => todo!(),
                // NativeActivityEvent::NativeWindowDestroyed(_) => todo!(),
                // NativeActivityEvent::InputQueueCreated(_) => todo!(),
                // NativeActivityEvent::InputQueueDestroyed(_) => todo!(),
                // NativeActivityEvent::ContentRectChanged(_) => todo!(),
                // NativeActivityEvent::ConfigurationChanged => todo!(),
                // NativeActivityEvent::LowMemory => todo!(),
                _ => (),
            },
            Err(e) => match e {
                TryRecvError::Empty => {}
                TryRecvError::Disconnected => {
                    if destroy_called {
                        info!("Exiting loop");
                        return Ok(()); // Normal loop exit
                    } else {
                        anyhow::bail!("Message channel improperly closed");
                    }
                }
            },
        };
    }
}

enum NativeActivityEvent {
    Start(*mut ANativeActivity),
    Resume(*mut ANativeActivity),
    SaveInstanceState(*mut c_ulong),
    Pause,
    Stop,
    Destroy,
    WindowFocusChanged(c_int),
    NativeWindowCreated(*mut ANativeWindow),
    NativeWindowResized(*mut ANativeWindow),
    NativeWindowRedrawNeeded(*mut ANativeWindow),
    NativeWindowDestroyed(*mut ANativeWindow),
    InputQueueCreated(*mut AInputQueue),
    InputQueueDestroyed(*mut AInputQueue),
    ContentRectChanged(*const ARect),
    ConfigurationChanged,
    LowMemory,
}

unsafe impl Send for NativeActivityEvent {}

macro_rules! send_event {
    ($activity:ident, $arg:expr) => {
        let ptr: *mut Sender<NativeActivityEvent> = (&*$activity).instance.cast();
        let sender = &mut *ptr;
        sender.send($arg).unwrap();
    };
}

unsafe extern "C" fn on_start(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::Start(activity));
}

unsafe extern "C" fn on_resume(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::Resume(activity));
}

unsafe extern "C" fn on_save_instance_state(
    activity: *mut ANativeActivity,
    out_size: *mut c_ulong,
) -> *mut c_void {
    send_event!(activity, NativeActivityEvent::SaveInstanceState(out_size));
    // TODO
    std::ptr::null_mut()
}

unsafe extern "C" fn on_pause(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::Pause);
}

unsafe extern "C" fn on_stop(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::Stop);
}

unsafe extern "C" fn on_destroy(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::Destroy);
    // destroy sender
    let ptr: *mut Sender<NativeActivityEvent> = (&*activity).instance.cast();
    let _ = Box::from_raw(ptr);
}

unsafe extern "C" fn on_window_focus_changed(activity: *mut ANativeActivity, has_focus: c_int) {
    send_event!(activity, NativeActivityEvent::WindowFocusChanged(has_focus));
}

unsafe extern "C" fn on_native_window_created(
    activity: *mut ANativeActivity,
    window: *mut ANativeWindow,
) {
    send_event!(activity, NativeActivityEvent::NativeWindowCreated(window));
}

unsafe extern "C" fn on_native_window_resized(
    activity: *mut ANativeActivity,
    window: *mut ANativeWindow,
) {
    send_event!(activity, NativeActivityEvent::NativeWindowResized(window));
}

unsafe extern "C" fn on_native_window_redraw_needed(
    activity: *mut ANativeActivity,
    window: *mut ANativeWindow,
) {
    send_event!(
        activity,
        NativeActivityEvent::NativeWindowRedrawNeeded(window)
    );
}

unsafe extern "C" fn on_native_window_destroyed(
    activity: *mut ANativeActivity,
    window: *mut ANativeWindow,
) {
    send_event!(activity, NativeActivityEvent::NativeWindowDestroyed(window));
}

unsafe extern "C" fn on_input_queue_created(
    activity: *mut ANativeActivity,
    queue: *mut AInputQueue,
) {
    send_event!(activity, NativeActivityEvent::InputQueueCreated(queue));
}

unsafe extern "C" fn on_input_queue_destroyed(
    activity: *mut ANativeActivity,
    queue: *mut AInputQueue,
) {
    send_event!(activity, NativeActivityEvent::InputQueueDestroyed(queue));
}

unsafe extern "C" fn on_content_rect_changed(activity: *mut ANativeActivity, rect: *const ARect) {
    send_event!(activity, NativeActivityEvent::ContentRectChanged(rect));
}

unsafe extern "C" fn on_configuration_changed(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::ConfigurationChanged);
}

unsafe extern "C" fn on_low_memory(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::LowMemory);
}
