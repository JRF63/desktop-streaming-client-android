use super::{MediaStatus, MimeType};
use ndk_sys::{
    AMediaFormat, AMediaFormat_delete, AMediaFormat_new, AMediaFormat_setInt32,
    AMediaFormat_setString, AMEDIAFORMAT_KEY_HEIGHT, AMEDIAFORMAT_KEY_MAX_HEIGHT,
    AMEDIAFORMAT_KEY_MAX_WIDTH, AMEDIAFORMAT_KEY_MIME, AMEDIAFORMAT_KEY_PRIORITY,
    AMEDIAFORMAT_KEY_WIDTH,
};
use std::ptr::NonNull;

// Only available starting API level 30
const MEDIAFORMAT_KEY_LOW_LATENCY: &'static str = "low-latency\0";

/// RAII wrapper for [AMediaFormat].
#[repr(transparent)]
pub struct MediaFormat(NonNull<AMediaFormat>);

impl Drop for MediaFormat {
    fn drop(&mut self) {
        unsafe {
            AMediaFormat_delete(self.0.as_ptr());
        }
    }
}

// FIXME: Is this safe?
unsafe impl Send for MediaFormat {}

impl MediaFormat {
    /// Create a new `MediaFormat`.
    pub fn new() -> Result<MediaFormat, MediaStatus> {
        let ptr = unsafe { AMediaFormat_new() };
        match NonNull::new(ptr) {
            Some(media_format) => Ok(MediaFormat(media_format)),
            None => Err(MediaStatus::AllocationError),
        }
    }

    /// Convert to an Android NDK [AMediaFormat] pointer.
    pub fn as_inner(&self) -> *mut AMediaFormat {
        self.0.as_ptr()
    }

    /// Sets the mime type.
    pub fn set_mime_type(&mut self, mime_type: MimeType) {
        unsafe {
            AMediaFormat_setString(
                self.as_inner(),
                AMEDIAFORMAT_KEY_MIME,
                mime_type.to_android_cstr().as_ptr().cast(),
            );
        }
    }

    /// Sets the resolution of the format.
    pub fn set_resolution(&mut self, width: i32, height: i32) {
        unsafe {
            AMediaFormat_setInt32(self.as_inner(), AMEDIAFORMAT_KEY_WIDTH, width);
            AMediaFormat_setInt32(self.as_inner(), AMEDIAFORMAT_KEY_HEIGHT, height);
        }
    }

    /// Sets the max resolution of the format. Used for adaptive playback.
    pub fn set_max_resolution(&mut self, width: i32, height: i32) {
        unsafe {
            AMediaFormat_setInt32(self.as_inner(), AMEDIAFORMAT_KEY_MAX_WIDTH, width);
            AMediaFormat_setInt32(self.as_inner(), AMEDIAFORMAT_KEY_MAX_HEIGHT, height);
        }
    }

    /// Sets the codec priority to be realtime or not. Added in API level 23.
    pub fn set_realtime_priority(&mut self, realtime: bool) {
        unsafe {
            AMediaFormat_setInt32(
                self.as_inner(),
                AMEDIAFORMAT_KEY_PRIORITY,
                if realtime { 0 } else { 1 },
            );
        }
    }

    /// Sets whether or not to enable low latency mode. Added in API level 30.
    pub fn set_low_latency(&mut self, low_latency: bool) {
        unsafe {
            AMediaFormat_setInt32(
                self.as_inner(),
                MEDIAFORMAT_KEY_LOW_LATENCY.as_ptr().cast(),
                if low_latency { 1 } else { 0 },
            );
        }
    }

    pub fn set_integer(&mut self, key: &str, val: i32) {
        use std::ffi::CString;

        if let Ok(cstring) = CString::new(key) {
            unsafe {
                AMediaFormat_setInt32(self.as_inner(), cstring.as_ptr().cast(), val);
            }
        }
    }
}
