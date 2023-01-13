mod debug;
mod log;
mod media;
mod window;

use jni::{objects::GlobalRef, JNIEnv, JavaVM};
use std::{
    sync::{Arc, Barrier},
    thread,
};

// adb logcat -v raw -s client-android
// adb install target\debug\apk\client-android.apk
// C:\Users\Rafael\AppData\Local\Android\Sdk\emulator\emulator -avd Pixel_3_XL_API_31
// adb install app\build\outputs\apk\debug\app-debug.apk
// gradlew installX86_64Debug
// set CC=C:\Users\Rafael\AppData\Local\Android\Sdk\ndk-bundle\toolchains\llvm\prebuilt\windows-x86_64\bin\aarch64-linux-android26-clang.cmd
// set AR=C:\Users\Rafael\AppData\Local\Android\Sdk\ndk-bundle\toolchains\llvm\prebuilt\windows-x86_64\bin\aarch64-linux-android-ar.exe

#[export_name = "Java_com_debug_myapplication_StreamingActivity_a"]
pub extern "system" fn create_native_instance(
    env: JNIEnv,
    activity: jni::sys::jobject,
    previous_instance: jni::sys::jlong,
) -> jni::sys::jlong {
    const NUM_THREADS: usize = 2;

    fn inner_fn(
        env: JNIEnv,
        activity: jni::sys::jobject,
        previous_instance: jni::sys::jlong,
    ) -> anyhow::Result<jni::sys::jlong> {
        if previous_instance == 0 {
            let android_activity =
                AndroidActivity::new(env.get_java_vm()?, env.new_global_ref(activity)?);

            let (decode_loop_tx, decode_loop_rx) = crossbeam_channel::bounded(3);
            let barrier = Arc::new(Barrier::new(NUM_THREADS));

            {
                let barrier_clone = barrier.clone();
                thread::spawn(move || {
                    if let Err(e) = decode_loop(decode_loop_rx, android_activity) {
                        error!("Decode loop error: {}", e);
                    }
                    barrier_clone.wait();
                });
            }

            let instance = Box::new(NativeInstance::new(decode_loop_tx, barrier));
            Ok(instance.into_java_long())
        } else {
            let native_instance = unsafe { NativeInstance::from_java_long(previous_instance) };
            let _ignored = native_instance.send_to_decoder(ActivityEvent::Create);
            Ok(previous_instance)
        }
    }
    match inner_fn(env, activity, previous_instance) {
        Ok(instance) => instance,
        Err(e) => {
            error!("Native instance creation error: {}", e);
            0
        }
    }
}

#[export_name = "Java_com_debug_myapplication_StreamingActivity_b"]
pub extern "system" fn send_destroy_signal(
    _env: JNIEnv,
    _activity: jni::sys::jobject,
    instance: jni::sys::jlong,
) {
    let native_instance = unsafe { NativeInstance::from_java_long(instance) };
    let _ignored = native_instance.send_to_decoder(ActivityEvent::Destroy);
    native_instance.barrier.wait();
    unsafe { NativeInstance::drop_instance(instance) }
}

#[export_name = "Java_com_debug_myapplication_StreamingActivity_c"]
pub extern "system" fn send_surface_created(
    env: JNIEnv,
    _activity: jni::sys::jobject,
    instance: jni::sys::jlong,
    surface: jni::sys::jobject,
) {
    fn inner_fn(
        env: JNIEnv,
        instance: jni::sys::jlong,
        surface: jni::sys::jobject,
    ) -> anyhow::Result<()> {
        let native_instance = unsafe { NativeInstance::from_java_long(instance) };
        let surface = env.new_global_ref(surface)?;
        native_instance.send_to_decoder(ActivityEvent::SurfaceCreated(surface))?;
        Ok(())
    }

    if let Err(e) = inner_fn(env, instance, surface) {
        error!("Sending `SurfaceCreated` failed: {}", e);
    }
}

#[export_name = "Java_com_debug_myapplication_StreamingActivity_d"]
pub extern "system" fn send_surface_destroyed(
    _env: JNIEnv,
    _activity: jni::sys::jobject,
    instance: jni::sys::jlong,
) {
    fn inner_fn(instance: jni::sys::jlong) -> anyhow::Result<()> {
        let native_instance = unsafe { NativeInstance::from_java_long(instance) };
        native_instance.send_to_decoder(ActivityEvent::SurfaceDestroyed)?;
        Ok(())
    }

    if let Err(e) = inner_fn(instance) {
        error!("Sending `SurfaceDestroyed` failed: {}", e);
    }
}

#[derive(Clone)]
enum ActivityEvent {
    Create,
    Destroy,
    SurfaceCreated(GlobalRef),
    SurfaceDestroyed,
}

struct NativeInstance {
    event_sender: crossbeam_channel::Sender<ActivityEvent>,
    barrier: Arc<Barrier>,
}

impl NativeInstance {
    fn new(
        decode_loop_sender: crossbeam_channel::Sender<ActivityEvent>,
        barrier: Arc<Barrier>,
    ) -> Self {
        NativeInstance {
            event_sender: decode_loop_sender,
            barrier,
        }
    }

    fn into_java_long(self: Box<NativeInstance>) -> jni::sys::jlong {
        let leaked_ptr = Box::into_raw(self);
        leaked_ptr as usize as jni::sys::jlong
    }

    unsafe fn from_java_long<'a>(instance: jni::sys::jlong) -> &'a Self {
        &*(instance as usize as *const NativeInstance)
    }

    unsafe fn drop_instance(instance: jni::sys::jlong) {
        let to_drop = Box::from_raw(instance as usize as *mut NativeInstance);
        std::mem::drop(to_drop);
    }

    fn send_to_decoder(
        &self,
        event: ActivityEvent,
    ) -> Result<(), crossbeam_channel::SendError<ActivityEvent>> {
        self.event_sender.send(event)
    }
}

struct AndroidActivity {
    vm: JavaVM,
    activity_obj: GlobalRef,
}

impl AndroidActivity {
    fn new(vm: JavaVM, activity_obj: GlobalRef) -> Self {
        Self { vm, activity_obj }
    }
}

fn decode_loop(
    event_receiver: crossbeam_channel::Receiver<ActivityEvent>,
    activity: AndroidActivity,
) -> anyhow::Result<()> {
    let env = activity.vm.attach_current_thread()?;

    let java_surface = loop {
        match event_receiver.recv() {
            Ok(msg) => match msg {
                ActivityEvent::Create => {
                    anyhow::bail!("Unexpected state change while waiting for a `Surface`")
                }
                ActivityEvent::SurfaceCreated(java_surface) => break java_surface,
                _ => anyhow::bail!("Received exit message before receiving a `Surface`"),
            },
            Err(_) => anyhow::bail!("Error in event channel while waiting for a `Surface`"),
        }
    };

    let mut native_window = window::NativeWindow::new(&env, &java_surface.as_obj())
        .ok_or_else(|| anyhow::anyhow!("Unable to acquire an `ANativeWindow`"))?;

    let width = 1920;
    let height = 1080;
    let decoder = media::MediaCodec::create_video_decoder(
        &native_window,
        media::VideoType::H264,
        width,
        height,
        60,
        debug::CSD,
    )?;
    info!("created decoder");

    let aspect_ratio_string = env.new_string(media::aspect_ratio_string(width, height))?;
    let obj = activity.activity_obj.as_obj();
    env.call_method(
        obj,
        "setSurfaceViewAspectRatio",
        "(Ljava/lang/String;)V",
        &[aspect_ratio_string.into()],
    )?;

    let mut time = 0;
    let mut packet_index = 0;

    loop {
        loop {
            match event_receiver.try_recv() {
                Ok(msg) => {
                    match msg {
                        ActivityEvent::Create => {
                            anyhow::bail!("Unexpected state change to `OnCreate` while inside the decoding loop")
                        }
                        ActivityEvent::Destroy => {
                            info!("`Destroy` was signaled before `SurfaceDestroyed`")
                        }
                        ActivityEvent::SurfaceCreated(_java_surface) => {
                            anyhow::bail!("Surface was re-created while inside the decoding loop")
                        }
                        ActivityEvent::SurfaceDestroyed => break,
                    }
                }
                Err(e) => match e {
                    crossbeam_channel::TryRecvError::Empty => {
                        if packet_index < 120 {
                            let end_of_stream = if packet_index == 119 { true } else { false };
                            if decoder.try_decode(
                                debug::PACKETS[packet_index],
                                time,
                                end_of_stream,
                            )? {
                                time += 16_666;
                                packet_index += 1;
                            }
                        }
                        decoder.try_render()?;
                    }
                    crossbeam_channel::TryRecvError::Disconnected => {
                        anyhow::bail!("Event channel was improperly dropped")
                    }
                },
            };
        }

        // Wait for `OnCreate` or `OnDestroy` event from Java side
        loop {
            match event_receiver.recv() {
                // Continue from `OnPause` or `OnStop`
                Ok(ActivityEvent::Create) => {
                    // Wait for a new surface to be created
                    if let Ok(ActivityEvent::SurfaceCreated(java_surface)) = event_receiver.recv() {
                        native_window = window::NativeWindow::new(&env, &java_surface.as_obj())
                            .ok_or_else(|| {
                                anyhow::anyhow!("Unable to acquire an `ANativeWindow`")
                            })?;
                        decoder.set_output_surface(&native_window)?;
                    }
                }
                // App is being terminated
                Ok(ActivityEvent::Destroy) => return Ok(()),
                Ok(_) => anyhow::bail!("Unexpected state change while waiting for `Create` signal"),
                Err(_) => anyhow::bail!("Event channel was improperly dropped"),
            }
        }
    }
}
