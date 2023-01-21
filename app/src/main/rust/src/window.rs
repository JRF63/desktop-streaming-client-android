use jni::{objects::JObject, JNIEnv};
use ndk_sys::{ANativeWindow, ANativeWindow_fromSurface, ANativeWindow_release};
use std::ptr::NonNull;

/// RAII wrapper around [ANativeWindow].
#[repr(transparent)]
pub struct NativeWindow(NonNull<ANativeWindow>);

impl Drop for NativeWindow {
    fn drop(&mut self) {
        unsafe {
            ANativeWindow_release(self.0.as_ptr());
        }
    }
}

impl NativeWindow {
    /// Create a `NativeWindow` from the surface received from the `surfaceCreated` event.
    pub fn new(env: &JNIEnv, surface: &JObject) -> Option<Self> {
        NonNull::new(unsafe {
            ANativeWindow_fromSurface(env.get_native_interface(), surface.into_raw())
        })
        .map(|ptr| NativeWindow(ptr))
    }

    /// Convert to an Android NDK [ANativeWindow] pointer.
    pub fn as_inner(&self) -> *mut ANativeWindow {
        self.0.as_ptr()
    }
}
