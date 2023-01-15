mod video;

use super::status::{AsMediaStatus, MediaStatus};
use ndk_sys::{
    AMediaFormat, AMediaFormat_delete, AMediaFormat_getString, AMediaFormat_new,
    AMediaFormat_setBuffer, AMediaFormat_setInt32, AMediaFormat_setString, AMEDIAFORMAT_KEY_HEIGHT,
    AMEDIAFORMAT_KEY_MAX_HEIGHT, AMEDIAFORMAT_KEY_MAX_WIDTH, AMEDIAFORMAT_KEY_MIME,
    AMEDIAFORMAT_KEY_PRIORITY, AMEDIAFORMAT_KEY_WIDTH,
};
use std::{
    ffi::{c_char, CStr},
    ptr::NonNull,
};
pub use self::video::{VideoType, H264Csd, HevcCsd};

// Need to put the strings here because `AMEDIAFORMAT_KEY_CSD_0` and `AMEDIAFORMAT_KEY_CSD_1`
// only became available in API level 28.
const MEDIAFORMAT_KEY_CSD_0: &'static str = "csd-0\0";
const MEDIAFORMAT_KEY_CSD_1: &'static str = "csd-1\0";

const H264_MIME_TYPE: &'static str = "video/avc\0";
const HEVC_MIME_TYPE: &'static str = "video/hevc\0";

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
    pub fn set_mime_type<T>(&mut self, mime_type: T)
    where
        T: MediaFormatMimeType,
    {
        unsafe {
            AMediaFormat_setString(
                self.as_inner(),
                AMEDIAFORMAT_KEY_MIME,
                mime_type.mime_type().as_ptr(),
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

    /// Helper function for adding data to the `MediaFormat` from a slice.
    pub fn set_buffer(&mut self, name: *const c_char, data: &[u8]) {
        unsafe {
            AMediaFormat_setBuffer(self.as_inner(), name, data.as_ptr().cast(), data.len() as _)
        }
    }

    /// Add extra data to the `MediaFormat`.
    pub fn add_data<T>(&mut self, data: T)
    where
        T: MediaFormatData,
    {
        data.add_to_media_format(self);
    }

    pub(crate) fn get_mime_type(&self) -> *const c_char {
        unsafe {
            // Resulting string is owned by the `AMediaFormat`
            let mut cstr: *const c_char = std::ptr::null();
            AMediaFormat_getString(self.as_inner(), AMEDIAFORMAT_KEY_MIME, &mut cstr);
            cstr
        }
    }
}

/// Trait encapsulating types that have a MIME type.
pub trait MediaFormatMimeType {
    // MIME type as a C string.
    fn mime_type(&self) -> &CStr;
}

/// Data that can be added to `MediaFormat`.
pub trait MediaFormatData {
    /// Include the data in the format.
    fn add_to_media_format(&self, media_format: &mut MediaFormat);
}
