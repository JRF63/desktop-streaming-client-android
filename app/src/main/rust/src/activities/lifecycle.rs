use jni::{objects::GlobalRef, JNIEnv};
use tokio::{
    runtime::{self, Runtime},
    sync::broadcast,
};

pub const RUNTIME_WORKER_THREADS: usize = 2;
pub const BROADCAST_CHANNEL_CAPACITY: usize = 8;

/// Events that are of interest to video decoding.
#[derive(Clone)]
pub enum ActivityEvent {
    MainActivityDestroyed,
    MediaPlayerCreated(GlobalRef),
    MediaPlayerDestroyed,
    SurfaceCreated(GlobalRef),
    SurfaceDestroyed,
}

impl std::fmt::Debug for ActivityEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MainActivityDestroyed => write!(f, "MainActivityDestroyed"),
            Self::MediaPlayerCreated(_) => write!(f, "MediaPlayerCreated"),
            Self::MediaPlayerDestroyed => write!(f, "MediaPlayerDestroyed"),
            Self::SurfaceCreated(_) => write!(f, "SurfaceCreated"),
            Self::SurfaceDestroyed => write!(f, "SurfaceDestroyed"),
        }
    }
}

/// Thread loop that runs the native code.
pub struct NativeInstance {
    runtime: Runtime,
    sender: broadcast::Sender<ActivityEvent>,
}

impl Drop for NativeInstance {
    fn drop(&mut self) {
        self.signal_event(ActivityEvent::MainActivityDestroyed);
    }
}

impl NativeInstance {
    /// Start a `NativeInstance`.
    pub fn new() -> Result<NativeInstance, std::io::Error> {
        let runtime = runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(RUNTIME_WORKER_THREADS)
            .build()?;
        let (sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);

        Ok(NativeInstance { runtime, sender })
    }

    /// Signal an `ActivityEvent`.
    pub fn signal_event(&self, event: ActivityEvent) {
        if let Err(e) = self.sender.send(event) {
            crate::error!("{e}");
        }
    }

    /// Reinterpret a [Box] as a 64-bit integer which can be stored in Kotlin/Java.
    pub fn into_java_long(self: Box<NativeInstance>) -> jni::sys::jlong {
        let leaked_ptr = Box::into_raw(self);
        leaked_ptr as usize as jni::sys::jlong
    }

    /// Convert a previously stored integer back into a `NativeInstance`. The value of `instance`
    /// is no longer valid after this call.
    pub unsafe fn from_raw_integer(instance: jni::sys::jlong) -> Box<NativeInstance> {
        Box::from_raw(instance as usize as *mut NativeInstance)
    }

    /// Reinterpret an integer to a reference to a `NativeInstance` without taking ownership.
    pub unsafe fn as_ref<'a>(instance: jni::sys::jlong) -> &'a Self {
        &*(instance as usize as *const NativeInstance)
    }
}

/// (Re)-initializes the native library. Should be called on `MainActivity`'s `onCreate`.
#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_createNativeInstance"]
pub extern "system" fn create_native_instance(
    _env: JNIEnv,
    _activity: jni::sys::jobject,
    instance: jni::sys::jlong,
) -> jni::sys::jlong {
    debug_assert_eq!(instance, 0);

    match NativeInstance::new() {
        Ok(instance) => Box::new(instance).into_java_long(),
        Err(e) => {
            crate::error!("Error creating native instance: {e}");
            0
        }
    }
}

#[export_name = "Java_com_debug_myapplication_NativeLibSingleton_destroyNativeInstance"]
pub extern "system" fn destroy_native_instance(
    _env: JNIEnv,
    _activity: jni::sys::jobject,
    instance: jni::sys::jlong,
) {
    debug_assert_ne!(instance, 0);
}
