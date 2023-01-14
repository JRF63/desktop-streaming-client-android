mod activity;
mod activities;
mod debug;
mod decoder;
mod lifecycle;
mod log;
mod media;
mod window;

use self::{
    activity::AndroidActivity,
    lifecycle::{ActivityEvent, NativeInstance},
};
use jni::JNIEnv;

// adb logcat -v raw -s client-android
// C:\Users\Rafael\AppData\Local\Android\Sdk\emulator\emulator -avd Pixel_3_XL_API_31
// gradlew installX86_64Debug

/// (Re)-initializes the native instance. Should be called on `onCreate`.
#[export_name = "Java_com_debug_myapplication_StreamingActivity_a"]
pub extern "system" fn create_native_instance(
    env: JNIEnv,
    activity: jni::sys::jobject,
    previous_instance: jni::sys::jlong,
) -> jni::sys::jlong {
    // If there is a running instance, return it back after signaling `onCreate`.
    if previous_instance != 0 {
        let native_instance = unsafe { NativeInstance::as_ref(previous_instance) };
        native_instance.signal_event(ActivityEvent::Create);
        previous_instance

    // else create a new native instance
    } else {
        match AndroidActivity::new(&env, activity) {
            Ok(android_activity) => {
                let instance = NativeInstance::new(move |receiver| {
                    // TODO
                    dummy_loop(receiver, android_activity);
                });
                let instance = Box::new(instance);
                instance.into_java_long()
            }
            Err(e) => {
                crate::error!("{e}");
                0
            }
        }
    }
}

/// `onDestroy` handler.
#[export_name = "Java_com_debug_myapplication_StreamingActivity_b"]
pub extern "system" fn on_destroy(
    _env: JNIEnv,
    _activity: jni::sys::jobject,
    instance: jni::sys::jlong,
) {
    let native_instance = unsafe { NativeInstance::from_raw_integer(instance) };
    native_instance.signal_event(ActivityEvent::Destroy);
    native_instance.join();
}

/// `surfaceCreated` handler.
#[export_name = "Java_com_debug_myapplication_StreamingActivity_c"]
pub extern "system" fn surface_created(
    env: JNIEnv,
    _activity: jni::sys::jobject,
    instance: jni::sys::jlong,
    surface: jni::sys::jobject,
) {
    let native_instance = unsafe { NativeInstance::as_ref(instance) };
    match env.new_global_ref(surface) {
        Ok(surface) => native_instance.signal_event(ActivityEvent::SurfaceCreated(surface)),
        Err(e) => crate::error!("On surfaceCreated handler: {e}"),
    }
}

/// `surfaceDestroyed` handler.
#[export_name = "Java_com_debug_myapplication_StreamingActivity_d"]
pub extern "system" fn surface_destroyed(
    _env: JNIEnv,
    _activity: jni::sys::jobject,
    instance: jni::sys::jlong,
) {
    let native_instance = unsafe { NativeInstance::as_ref(instance) };
    native_instance.signal_event(ActivityEvent::SurfaceDestroyed);
}

fn dummy_loop(
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<ActivityEvent>,
    _activity: AndroidActivity,
) {
    crate::info!("dummy_loop");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Unable to create tokio runtime");
    runtime.block_on(async move {
        while let Some(msg) = receiver.recv().await {
            crate::info!("Message: {msg:?}");
            match msg {
                ActivityEvent::Create => (),
                ActivityEvent::Destroy => break,
                ActivityEvent::SurfaceCreated(_) => (),
                ActivityEvent::SurfaceDestroyed => (),
            }
        }
    });
}

// fn decode_loop(
//     event_receiver: crossbeam_channel::Receiver<ActivityEvent>,
//     activity: AndroidActivity,
// ) -> anyhow::Result<()> {
//     let env = activity.vm.attach_current_thread()?;

//     let java_surface = loop {
//         match event_receiver.recv() {
//             Ok(msg) => match msg {
//                 ActivityEvent::Create => {
//                     anyhow::bail!("Unexpected state change while waiting for a `Surface`")
//                 }
//                 ActivityEvent::SurfaceCreated(java_surface) => break java_surface,
//                 _ => anyhow::bail!("Received exit message before receiving a `Surface`"),
//             },
//             Err(_) => anyhow::bail!("Error in event channel while waiting for a `Surface`"),
//         }
//     };

//     let mut native_window = window::NativeWindow::new(&env, &java_surface.as_obj())
//         .ok_or_else(|| anyhow::anyhow!("Unable to acquire an `ANativeWindow`"))?;

//     let width = 1920;
//     let height = 1080;
//     let decoder = media::MediaCodec::create_video_decoder(
//         &native_window,
//         media::VideoType::H264,
//         width,
//         height,
//         60,
//         debug::CSD,
//     )?;
//     info!("created decoder");

//     let aspect_ratio_string = env.new_string(media::aspect_ratio_string(width, height))?;
//     let obj = activity.activity_obj.as_obj();
//     env.call_method(
//         obj,
//         "setSurfaceViewAspectRatio",
//         "(Ljava/lang/String;)V",
//         &[aspect_ratio_string.into()],
//     )?;

//     let mut time = 0;
//     let mut packet_index = 0;

//     loop {
//         loop {
//             match event_receiver.try_recv() {
//                 Ok(msg) => {
//                     match msg {
//                         ActivityEvent::Create => {
//                             anyhow::bail!("Unexpected state change to `OnCreate` while inside the decoding loop")
//                         }
//                         ActivityEvent::Destroy => {
//                             info!("`Destroy` was signaled before `SurfaceDestroyed`")
//                         }
//                         ActivityEvent::SurfaceCreated(_java_surface) => {
//                             anyhow::bail!("Surface was re-created while inside the decoding loop")
//                         }
//                         ActivityEvent::SurfaceDestroyed => break,
//                     }
//                 }
//                 Err(e) => match e {
//                     crossbeam_channel::TryRecvError::Empty => {
//                         if packet_index < 120 {
//                             let end_of_stream = if packet_index == 119 { true } else { false };
//                             if decoder.try_decode(
//                                 debug::PACKETS[packet_index],
//                                 time,
//                                 end_of_stream,
//                             )? {
//                                 time += 16_666;
//                                 packet_index += 1;
//                             }
//                         }
//                         decoder.try_render()?;
//                     }
//                     crossbeam_channel::TryRecvError::Disconnected => {
//                         anyhow::bail!("Event channel was improperly dropped")
//                     }
//                 },
//             };
//         }

//         // Wait for `OnCreate` or `OnDestroy` event from Java side
//         loop {
//             match event_receiver.recv() {
//                 // Continue from `OnPause` or `OnStop`
//                 Ok(ActivityEvent::Create) => {
//                     // Wait for a new surface to be created
//                     if let Ok(ActivityEvent::SurfaceCreated(java_surface)) = event_receiver.recv() {
//                         native_window = window::NativeWindow::new(&env, &java_surface.as_obj())
//                             .ok_or_else(|| {
//                                 anyhow::anyhow!("Unable to acquire an `ANativeWindow`")
//                             })?;
//                         decoder.set_output_surface(&native_window)?;
//                     }
//                 }
//                 // App is being terminated
//                 Ok(ActivityEvent::Destroy) => return Ok(()),
//                 Ok(_) => anyhow::bail!("Unexpected state change while waiting for `Create` signal"),
//                 Err(_) => anyhow::bail!("Event channel was improperly dropped"),
//             }
//         }
//     }
// }
