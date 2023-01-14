use jni::objects::GlobalRef;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// Events that are of interest to video decoding.
#[derive(Clone)]
pub enum ActivityEvent {
    Create,
    Destroy,
    SurfaceCreated(GlobalRef),
    SurfaceDestroyed,
}

impl std::fmt::Debug for ActivityEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create => write!(f, "Create"),
            Self::Destroy => write!(f, "Destroy"),
            Self::SurfaceCreated(_) => write!(f, "SurfaceCreated"),
            Self::SurfaceDestroyed => write!(f, "SurfaceDestroyed"),
        }
    }
}

/// Thread loop that runs the native code.
pub struct NativeInstance {
    sender: UnboundedSender<ActivityEvent>,
    join_handle: std::thread::JoinHandle<()>,
}

impl NativeInstance {
    /// Start a `NativeInstance`.
    pub fn new<F>(func: F) -> NativeInstance
    where
        F: FnOnce(UnboundedReceiver<ActivityEvent>) + Send + 'static,
    {
        let (sender, receiver) = unbounded_channel();
        let join_handle = std::thread::spawn(move || {
            func(receiver);
        });
        NativeInstance {
            sender,
            join_handle,
        }
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

    /// Consumes the [Box] and wait for the job thread to finish.
    pub fn join(self: Box<NativeInstance>) {
        if self.join_handle.join().is_err() {
            crate::error!("Unable to join spawned thread");
        }
    }

    /// Reinterpret an integer to a reference to a `NativeInstance` without taking ownership.
    pub unsafe fn as_ref<'a>(instance: jni::sys::jlong) -> &'a Self {
        &*(instance as usize as *const NativeInstance)
    }
}
