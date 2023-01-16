mod debug;
mod log;
mod media;
mod network;
mod util;
mod window;

// adb logcat -v raw -s client-android
// C:\Users\Rafael\AppData\Local\Android\Sdk\emulator\emulator -avd Pixel_3_XL_API_31
// gradlew installX86_64Debug

use jni::{objects::GlobalRef, JNIEnv, JavaVM};
use std::future::Future;
use tokio::{
    runtime::{self, Runtime},
    sync::broadcast,
};

pub const RUNTIME_WORKER_THREADS: usize = 2;
pub const BROADCAST_CHANNEL_CAPACITY: usize = 8;

/// Events that are of interest to the media player.
#[derive(Clone)]
pub enum MediaPlayerEvent {
    MainActivityDestroyed,
    SurfaceCreated(GlobalRef),
    SurfaceDestroyed,
}

impl std::fmt::Debug for MediaPlayerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MainActivityDestroyed => write!(f, "MainActivityDestroyed"),
            Self::SurfaceCreated(_) => write!(f, "SurfaceCreated"),
            Self::SurfaceDestroyed => write!(f, "SurfaceDestroyed"),
        }
    }
}

/// Thread pool that runs the native code.
pub struct NativeLibManager {
    sender: broadcast::Sender<MediaPlayerEvent>,
    runtime: Runtime,
}

impl NativeLibManager {
    /// Create a `NativeLibManager`.
    pub fn new() -> Result<NativeLibManager, std::io::Error> {
        let runtime = runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(RUNTIME_WORKER_THREADS)
            .build()?;
        let (sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);

        Ok(NativeLibManager { sender, runtime })
    }

    /// Signal an `ActivityEvent`.
    pub fn signal_event(&self, event: MediaPlayerEvent) {
        if let Err(e) = self.sender.send(event) {
            crate::error!("{e}");
        }
    }

    /// Reinterpret a [Box] as a 64-bit integer which can be stored in Kotlin/Java.
    pub fn into_java_long(self: Box<NativeLibManager>) -> jni::sys::jlong {
        let leaked_ptr = Box::into_raw(self);
        leaked_ptr as usize as jni::sys::jlong
    }

    /// Convert a previously stored integer back into a `NativeLibManager`. The value of `instance`
    /// is no longer valid after this call.
    pub unsafe fn from_raw_integer(instance: jni::sys::jlong) -> Box<NativeLibManager> {
        Box::from_raw(instance as usize as *mut NativeLibManager)
    }

    /// Reinterpret an integer as a reference to a `NativeLibManager` without taking ownership.
    pub unsafe fn as_ref<'a>(instance: jni::sys::jlong) -> &'a Self {
        &*(instance as usize as *const NativeLibManager)
    }

    pub fn spawn_media_player<T, F>(&self, vm: JavaVM, singleton: GlobalRef, func: T)
    where
        T: FnOnce(JavaVM, GlobalRef, broadcast::Receiver<MediaPlayerEvent>) -> F,
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let receiver = self.sender.subscribe();
        self.runtime.spawn(func(vm, singleton, receiver));
    }
}

/// Initializes the native library.
#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_createNativeInstance"]
pub extern "system" fn create_native_instance(
    _env: JNIEnv,
    _singleton: jni::sys::jobject,
) -> jni::sys::jlong {
    match NativeLibManager::new() {
        Ok(instance) => Box::new(instance).into_java_long(),
        Err(e) => {
            crate::error!("Error creating native instance: {e}");
            0
        }
    }
}

/// Frees the native library.
#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_destroyNativeInstance"]
pub extern "system" fn destroy_native_instance(
    _env: JNIEnv,
    _singleton: jni::sys::jobject,
    ptr: jni::sys::jlong,
) {
    debug_assert_ne!(ptr, 0);
    let boxed = unsafe { NativeLibManager::from_raw_integer(ptr) };
    boxed.signal_event(MediaPlayerEvent::MainActivityDestroyed);
    let instance = *boxed;
    instance.runtime.shutdown_background();
}

#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_sendSurface"]
pub extern "system" fn send_surface(
    env: JNIEnv,
    _singleton: jni::sys::jobject,
    ptr: jni::sys::jlong,
    surface: jni::sys::jobject,
) {
    debug_assert_ne!(ptr, 0);
    let instance = unsafe { NativeLibManager::as_ref(ptr) };
    let surface = match env.new_global_ref(surface) {
        Ok(s) => s,
        Err(e) => {
            crate::error!("Error creating global ref: {e}");
            return;
        }
    };
    instance.signal_event(MediaPlayerEvent::SurfaceCreated(surface));
}

#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_destroySurface"]
pub extern "system" fn destroy_surface(
    _env: JNIEnv,
    _singleton: jni::sys::jobject,
    ptr: jni::sys::jlong,
) {
    debug_assert_ne!(ptr, 0);
    let instance = unsafe { NativeLibManager::as_ref(ptr) };
    instance.signal_event(MediaPlayerEvent::SurfaceDestroyed);
}

#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_startMediaPlayer"]
pub extern "system" fn start_media_player(
    env: JNIEnv,
    singleton: jni::sys::jobject,
    ptr: jni::sys::jlong,
) {
    debug_assert_ne!(ptr, 0);
    let instance = unsafe { NativeLibManager::as_ref(ptr) };

    let vm = match env.get_java_vm() {
        Ok(vm) => vm,
        Err(e) => {
            crate::error!("{e}");
            return;
        }
    };
    let singleton = match env.new_global_ref(singleton) {
        Ok(s) => s,
        Err(e) => {
            crate::error!("{e}");
            return;
        }
    };

    instance.spawn_media_player(vm, singleton, test_decode);
}

pub fn set_media_player_aspect_ratio(
    env: &JNIEnv,
    singleton: &GlobalRef,
    width: i32,
    height: i32,
) -> Result<(), jni::errors::Error> {
    // Reduce the given width:height ratio to an irreducible fraction.
    let (width, height) = {
        let divisor = crate::util::gcd(width, height);
        (width / divisor, height / divisor)
    };

    env.call_method(
        singleton.as_obj(),
        "setMediaPlayerAspectRatio",
        "(II)V",
        &[width.into(), height.into()],
    )?;
    Ok(())
}

async fn test_decode(
    vm: JavaVM,
    singleton: GlobalRef,
    receiver: broadcast::Receiver<MediaPlayerEvent>,
) {
    if let Err(e) = run_decoder(vm, singleton, receiver).await {
        println!("{e}");
    }
}

async fn run_decoder(
    vm: JavaVM,
    singleton: GlobalRef,
    mut receiver: broadcast::Receiver<MediaPlayerEvent>,
) -> anyhow::Result<()> {
    loop {
        match receiver.recv().await {
            Ok(msg) => match msg {
                MediaPlayerEvent::SurfaceCreated(java_surface) => {
                    let env = vm.attach_current_thread()?;

                    let native_window = window::NativeWindow::new(&env, &java_surface.as_obj())
                        .ok_or_else(|| anyhow::anyhow!("Unable to acquire a `ANativeWindow`"))?;

                    let width = 1920;
                    let height = 1080;

                    set_media_player_aspect_ratio(&env, &singleton, width, height)?;

                    let mut format = media::MediaFormat::new()?;
                    format.set_resolution(width, height);
                    format.set_max_resolution(width, height);
                    format.set_mime_type(media::VideoType::H264);
                    format.set_realtime_priority(true);

                    let mut decoder = media::MediaCodec::new_decoder(media::VideoType::H264)?;
                    decoder.initialize(&format, Some(native_window), false)?;

                    crate::info!("created decoder");

                    const FRAME_INTERVAL_MICROS: u64 = 16_666;
                    let dur = std::time::Duration::from_micros(FRAME_INTERVAL_MICROS);
                    let mut time = 0;

                    decoder.submit_codec_config(|buffer| {
                        let data = debug::CSD;
                        let min_len = data.len().min(buffer.len());
                        buffer[..min_len].copy_from_slice(&data[..min_len]);
                        (min_len, 0)
                    })?;

                    for packet_index in 0..119 {
                        crate::info!("decode: {packet_index}");
                        decoder.decode(|buffer| {
                            let data = debug::PACKETS[packet_index];
                            let min_len = data.len().min(buffer.len());
                            buffer[..min_len].copy_from_slice(&data[..min_len]);
                            (min_len, time)
                        })?;
                        time += FRAME_INTERVAL_MICROS;
                        decoder.render_output()?;
                        std::thread::sleep(dur);
                    }
                    decoder.decode(|buffer| {
                        let data = debug::PACKETS[119];
                        let min_len = data.len().min(buffer.len());
                        buffer[..min_len].copy_from_slice(&data[..min_len]);
                        (min_len, time)
                    })?;
                    decoder.render_output()?;
                }
                msg => anyhow::bail!("Unexpected message while waiting for a surface: {msg:?}"),
            },
            Err(e) => anyhow::bail!("Channel closed: {e}"),
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}
