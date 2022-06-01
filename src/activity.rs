use ndk_sys::{AInputQueue, ANativeActivity, ANativeWindow, ARect};
use std::{
    os::raw::{c_int, c_ulong, c_void},
    ptr::NonNull,
    sync::{Arc, Barrier},
};

/// New type for a `ANativeActivity`
#[repr(transparent)]
pub(crate) struct NativeActivity(ANativeActivity);

// Required for passing the pointer to the main loop
unsafe impl Send for NativeActivity {}

impl NativeActivity {
    /// Create a mutable reference to a `NativeActivity` from a raw pointer. The function does not
    /// check for the vailidity of the passed pointer.
    pub(crate) unsafe fn from_ptr<'a>(ptr: *mut ANativeActivity) -> &'a mut Self {
        NonNull::new(ptr.cast()).unwrap_unchecked().as_mut()
    }

    pub(crate) fn as_ndk_ptr(&self) -> *mut ANativeActivity {
        let ptr = self as *const NativeActivity as *mut NativeActivity;
        ptr.cast()
    }

    pub(crate) fn set_callbacks(&mut self) -> Option<()> {
        // SAFETY: Assuming the `callbacks` pointer points to a valid location
        let callbacks = unsafe { self.callbacks.as_mut()? };
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
        Some(())
    }
}

impl std::ops::Deref for NativeActivity {
    type Target = ANativeActivity;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.as_ndk_ptr() }
    }
}

impl std::ops::DerefMut for NativeActivity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.as_ndk_ptr() }
    }
}

pub(crate) struct NativeActivityInstance {
    event_sender: crossbeam_channel::Sender<NativeActivityEvent>,
    barrier: Arc<Barrier>,
}

impl NativeActivityInstance {
    unsafe fn from_ptr<'a>(ptr: *mut c_void) -> &'a Self {
        &*ptr.cast()
    }

    pub(crate) fn new(
        event_sender: crossbeam_channel::Sender<NativeActivityEvent>,
        barrier: Arc<Barrier>,
    ) -> Self {
        NativeActivityInstance {
            event_sender,
            barrier,
        }
    }

    pub(crate) unsafe fn drop_ptr(ptr: *mut c_void) {
        let _to_drop = Box::from_raw(ptr.cast::<NativeActivityInstance>());
    }
}

#[derive(Debug)]
pub(crate) enum NativeActivityEvent {
    Start,
    Resume,
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

// Required for sending pointers
unsafe impl Send for NativeActivityEvent {}

macro_rules! send_event {
    ($activity:ident, $arg:expr) => {
        let ptr = (&*$activity).instance;
        NativeActivityInstance::from_ptr(ptr)
            .event_sender
            .send($arg)
            .unwrap();
    };
}

unsafe extern "C" fn on_start(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::Start);
}

unsafe extern "C" fn on_resume(activity: *mut ANativeActivity) {
    send_event!(activity, NativeActivityEvent::Resume);
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
    let ptr = (&*activity).instance;
    NativeActivityInstance::from_ptr(ptr).barrier.wait();
    NativeActivityInstance::drop_ptr(ptr);
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
