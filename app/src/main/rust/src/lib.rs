// mod debug;
mod media;
mod util;
mod webrtc;
mod window;

// adb logcat -v raw -s client-android
// C:\Users\Rafael\AppData\Local\Android\Sdk\emulator\emulator -avd Pixel_3_XL_API_31
// gradlew installX86_64Debug

use self::media::MimeType;
use jni::{
    objects::{GlobalRef, JObject, JString, JValue, ReleaseMode},
    JNIEnv, JavaVM,
};
use std::{
    future::Future,
    sync::{Arc, Mutex},
};
use tokio::{
    runtime::{self, Runtime},
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};

pub const RUNTIME_WORKER_THREADS: usize = 2;

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

/// Mirror of the `NativeLibSingleton` in the Kotlin code. The two serves as a convenience bridge
/// for calling code across the languages.
///
/// This struct serves as a thread pool manager via the Tokio runtime that handles the async tasks.
pub struct NativeLibSingleton {
    vm: JavaVM,
    singleton: GlobalRef,
    runtime: Runtime,
    sender: UnboundedSender<MediaPlayerEvent>,
    receiver: Mutex<Option<UnboundedReceiver<MediaPlayerEvent>>>,
}

impl NativeLibSingleton {
    /// Create a `NativeLibManager`.
    pub fn new(vm: JavaVM, singleton: GlobalRef) -> Result<NativeLibSingleton, std::io::Error> {
        let runtime = runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(RUNTIME_WORKER_THREADS)
            .build()?;
        let (sender, receiver) = unbounded_channel();

        Ok(NativeLibSingleton {
            vm,
            singleton,
            runtime,
            sender,
            receiver: Mutex::new(Some(receiver)),
        })
    }

    /// Signal an `ActivityEvent`.
    pub fn signal_event(&self, event: MediaPlayerEvent) {
        if let Err(e) = self.sender.send(event) {
            log::error!("{e}");
        }
    }

    /// Reinterpret a [Arc] as a 64-bit integer which can be stored in Kotlin/Java.
    pub fn into_java_long(self: Arc<NativeLibSingleton>) -> jni::sys::jlong {
        let leaked_ptr = Arc::into_raw(self);
        leaked_ptr as usize as jni::sys::jlong
    }

    /// Convert a previously stored integer back into a `NativeLibManager`. The value of `instance`
    /// is no longer valid after this call.
    pub unsafe fn from_raw_integer(instance: jni::sys::jlong) -> Arc<NativeLibSingleton> {
        Arc::from_raw(instance as usize as *mut NativeLibSingleton)
    }

    /// Reinterpret an integer as a reference to a `NativeLibManager` without taking ownership.
    pub unsafe fn as_ref<'a>(instance: jni::sys::jlong) -> &'a Self {
        &*(instance as usize as *const NativeLibSingleton)
    }

    /// Returns the process-global Java VM.
    pub fn global_vm(&self) -> &JavaVM {
        &self.vm
    }

    /// Spawn an async function on the runtime.
    pub fn spawn<T, F>(self: &Arc<NativeLibSingleton>, func: T)
    where
        T: FnOnce(Arc<NativeLibSingleton>) -> F,
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime.spawn(func(self.clone()));
    }

    /// Get the receiver part of the `MediaPlayerEvent` channel.
    pub fn get_event_receiver(&self) -> Option<UnboundedReceiver<MediaPlayerEvent>> {
        let mut lock_guard = self.receiver.lock().ok()?;
        lock_guard.take()
    }

    /// Returns the API level of the device that this is currently running on.
    pub fn get_api_level(&self, env: &JNIEnv) -> Result<i32, jni::errors::Error> {
        let method_output = env.call_method(self.singleton.as_obj(), "getApiLevel", "()I", &[])?;

        match method_output {
            JValue::Int(level) => Ok(level),
            _ => Err(jni::errors::Error::JavaException),
        }
    }

    /// Call the singleton method to set the aspect ratio of the player.
    pub fn set_media_player_aspect_ratio(
        &self,
        env: &JNIEnv,
        width: i32,
        height: i32,
    ) -> Result<(), jni::errors::Error> {
        // Reduce the given width:height ratio to its lowest terms.
        let (width, height) = {
            let divisor = crate::util::gcd(width, height);
            (width / divisor, height / divisor)
        };

        env.call_method(
            self.singleton.as_obj(),
            "setMediaPlayerAspectRatio",
            "(II)V",
            &[width.into(), height.into()],
        )?;
        Ok(())
    }

    /// Choose a decoder for the given MIME type. The logic is handled on the Kotlin side.
    pub fn choose_decoder_for_type(
        &self,
        env: &JNIEnv,
        mime_type: MimeType,
    ) -> Result<Option<String>, jni::errors::Error> {
        let mime_type = env.new_string(mime_type.to_android_str())?;
        let method_output = env.call_method(
            self.singleton.as_obj(),
            "chooseDecoderForType",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[mime_type.into()],
        )?;
        
        let obj = method_output.l()?;
        if obj.into_raw().is_null() {
            return Ok(None);
        }

        let jstring = JString::from(obj);
        let java_str = env.get_string(jstring)?;
        let s = java_str
            .to_str()
            .map_err(|_| jni::errors::Error::JavaException)?;
        Ok(Some(s.to_owned()))
    }

    /// List the available codec profiles for the decoder.
    pub fn list_profiles_for_decoder(
        &self,
        env: &JNIEnv,
        decoder_name: &str,
        mime_type: MimeType,
    ) -> Result<Option<Vec<i32>>, jni::errors::Error> {
        let decoder_name = env.new_string(decoder_name)?;
        let mime_type = env.new_string(mime_type.to_android_str())?;
        let method_output = env.call_method(
            self.singleton.as_obj(),
            "listProfilesForDecoder",
            "(Ljava/lang/String;Ljava/lang/String;)[I",
            &[decoder_name.into(), mime_type.into()],
        )?;

        let obj = method_output.l()?;
        if obj.into_raw().is_null() {
            return Ok(None);
        }

        let array = env.get_int_array_elements(obj.into_raw(), ReleaseMode::NoCopyBack)?;
        let array_len = array.size()? as usize;
        let mut profiles = Vec::with_capacity(array_len);

        let ptr = array.as_ptr();
        for i in 0..array_len {
            profiles.push(unsafe { *ptr.offset(i as isize) });
        }
        Ok(Some(profiles))
    }
}

/// Initializes the native library.
#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_createNativeInstance"]
pub extern "system" fn create_native_instance(
    env: JNIEnv,
    singleton: jni::sys::jobject,
) -> jni::sys::jlong {
    let vm = match env.get_java_vm() {
        Ok(vm) => vm,
        Err(e) => {
            log::error!("{e}");
            return 0;
        }
    };

    debug_assert!(!singleton.is_null());
    let singleton = unsafe { JObject::from_raw(singleton) };
    let singleton = match env.new_global_ref(singleton) {
        Ok(s) => s,
        Err(e) => {
            log::error!("{e}");
            return 0;
        }
    };

    match NativeLibSingleton::new(vm, singleton) {
        Ok(instance) => {
            android_logger::init_once(
                android_logger::Config::default()
                    .with_min_level(log::Level::Info)
                    .with_tag("client-android"),
            );
            Arc::new(instance).into_java_long()
        }
        Err(e) => {
            log::error!("Error creating native instance: {e}");
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
    if ptr != 0 {
        let arc = unsafe { NativeLibSingleton::from_raw_integer(ptr) };
        arc.signal_event(MediaPlayerEvent::MainActivityDestroyed);
        std::mem::drop(arc); // Unnecessary but emphasizes that it will be dropped and freed
    }
}

/// Sends the `MediaPlayerActivity`'s `android.view.Surface` to the decoder.
#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_sendSurface"]
pub extern "system" fn send_surface(
    env: JNIEnv,
    _singleton: jni::sys::jobject,
    ptr: jni::sys::jlong,
    surface: jni::sys::jobject,
) {
    debug_assert_ne!(ptr, 0);
    let instance = unsafe { NativeLibSingleton::as_ref(ptr) };

    debug_assert!(!surface.is_null());
    let surface = unsafe { JObject::from_raw(surface) };
    let surface = match env.new_global_ref(surface) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Error creating global ref: {e}");
            return;
        }
    };
    instance.signal_event(MediaPlayerEvent::SurfaceCreated(surface));
}

/// Signal to the decoder that the previous `android.view.Surface` has been destroyed.
#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_destroySurface"]
pub extern "system" fn destroy_surface(
    _env: JNIEnv,
    _singleton: jni::sys::jobject,
    ptr: jni::sys::jlong,
) {
    debug_assert_ne!(ptr, 0);
    let instance = unsafe { NativeLibSingleton::as_ref(ptr) };
    instance.signal_event(MediaPlayerEvent::SurfaceDestroyed);
}

/// Start the WebRTC decoder.
#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_startMediaPlayer"]
pub extern "system" fn start_media_player(
    _env: JNIEnv,
    _singleton: jni::sys::jobject,
    ptr: jni::sys::jlong,
) {
    debug_assert_ne!(ptr, 0);

    log::info!("starting");

    let arc = unsafe { NativeLibSingleton::from_raw_integer(ptr) };
    arc.spawn(webrtc::start_webrtc);
    std::mem::forget(arc); // Prevent the `Arc` from being dropped
}
