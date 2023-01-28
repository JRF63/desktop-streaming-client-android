// mod debug;
mod log;
mod media;
mod util;
mod webrtc;
mod window;

// adb logcat -v raw -s client-android
// C:\Users\Rafael\AppData\Local\Android\Sdk\emulator\emulator -avd Pixel_3_XL_API_31
// gradlew installX86_64Debug

use jni::{
    objects::{GlobalRef, JObject, JString, JValue, ReleaseMode},
    JNIEnv, JavaVM,
};
use std::{future::Future, sync::Arc};
use tokio::{
    runtime::{self, Runtime},
    sync::broadcast,
};

pub const RUNTIME_WORKER_THREADS: usize = 2;
pub const BROADCAST_CHANNEL_CAPACITY: usize = 4;

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
pub struct NativeLibSingleton {
    vm: JavaVM,
    singleton: GlobalRef,
    runtime: Runtime,
    sender: broadcast::Sender<MediaPlayerEvent>,
}

impl NativeLibSingleton {
    /// Create a `NativeLibManager`.
    pub fn new(vm: JavaVM, singleton: GlobalRef) -> Result<NativeLibSingleton, std::io::Error> {
        let runtime = runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(RUNTIME_WORKER_THREADS)
            .build()?;
        let (sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);

        Ok(NativeLibSingleton {
            vm,
            singleton,
            runtime,
            sender,
        })
    }

    /// Signal an `ActivityEvent`.
    pub fn signal_event(&self, event: MediaPlayerEvent) {
        if let Err(e) = self.sender.send(event) {
            crate::error!("{e}");
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

    pub fn spawn<T, F>(self: &Arc<NativeLibSingleton>, func: T)
    where
        T: FnOnce(Arc<NativeLibSingleton>) -> F,
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime.spawn(func(self.clone()));
    }

    pub fn get_event_receiver(&self) -> broadcast::Receiver<MediaPlayerEvent> {
        self.sender.subscribe()
    }

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

    pub fn choose_decoder_for_type(
        &self,
        env: &JNIEnv,
        mime_type: &str,
    ) -> Result<Option<String>, jni::errors::Error> {
        let mime_type = env.new_string(mime_type)?;
        let method_output = env.call_method(
            self.singleton.as_obj(),
            "chooseDecoderForType",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[mime_type.into()],
        )?;

        let JValue::Object(obj) = method_output else {
            return Err(jni::errors::Error::JavaException);
        };
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

    pub fn list_profiles_for_decoder(
        &self,
        env: &JNIEnv,
        decoder_name: &str,
        mime_type: &str,
    ) -> Result<Option<Vec<i32>>, jni::errors::Error> {
        let decoder_name = env.new_string(decoder_name)?;
        let mime_type = env.new_string(mime_type)?;
        let method_output = env.call_method(
            self.singleton.as_obj(),
            "listProfilesForDecoder",
            "(Ljava/lang/String;Ljava/lang/String;)[I",
            &[decoder_name.into(), mime_type.into()],
        )?;

        let JValue::Object(obj) = method_output else {
            return Err(jni::errors::Error::JavaException);
        };
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
            crate::error!("{e}");
            return 0;
        }
    };

    debug_assert!(!singleton.is_null());
    let singleton = unsafe { JObject::from_raw(singleton) };
    let singleton = match env.new_global_ref(singleton) {
        Ok(s) => s,
        Err(e) => {
            crate::error!("{e}");
            return 0;
        }
    };

    match NativeLibSingleton::new(vm, singleton) {
        Ok(instance) => Arc::new(instance).into_java_long(),
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
    if ptr != 0 {
        let arc = unsafe { NativeLibSingleton::from_raw_integer(ptr) };
        arc.signal_event(MediaPlayerEvent::MainActivityDestroyed);
        std::mem::drop(arc); // Unnecessary but emphasizes that it will be dropped and freed
    }
}

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
    let instance = unsafe { NativeLibSingleton::as_ref(ptr) };
    instance.signal_event(MediaPlayerEvent::SurfaceDestroyed);
}

#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_startMediaPlayer"]
pub extern "system" fn start_media_player(
    _env: JNIEnv,
    _singleton: jni::sys::jobject,
    ptr: jni::sys::jlong,
) {
    debug_assert_ne!(ptr, 0);

    crate::info!("starting");

    let arc = unsafe { NativeLibSingleton::from_raw_integer(ptr) };
    arc.spawn(webrtc::start_webrtc);
    std::mem::forget(arc); // Prevent the `Arc` from being dropped
}
