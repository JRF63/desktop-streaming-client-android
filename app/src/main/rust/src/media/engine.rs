use super::{
    format::MediaFormat,
    status::{AsMediaStatus, MediaStatus},
};
use crate::window::NativeWindow;
use ndk_sys::{
    AMediaCodec, AMediaCodec_configure, AMediaCodec_createCodecByName, AMediaCodec_delete,
    AMediaCodec_dequeueInputBuffer, AMediaCodec_dequeueOutputBuffer, AMediaCodec_getInputBuffer,
    AMediaCodec_queueInputBuffer, AMediaCodec_releaseOutputBuffer, AMediaCodec_setOutputSurface,
    AMediaCodec_start, AMediaCodec_stop, AMEDIACODEC_BUFFER_FLAG_CODEC_CONFIG,
    AMEDIACODEC_CONFIGURE_FLAG_ENCODE, AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED,
    AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED, AMEDIACODEC_INFO_TRY_AGAIN_LATER,
};
use std::{
    ffi::{c_long, c_ulong, CString},
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    time::Duration,
};

/// Encapsulates a encoder/decoder.
#[repr(transparent)]
pub struct MediaEngine(NonNull<AMediaCodec>);

// FIXME: Is this safe?
unsafe impl Send for MediaEngine {}

impl Drop for MediaEngine {
    fn drop(&mut self) {
        unsafe {
            if let Err(e) = AMediaCodec_stop(self.as_inner()).success() {
                log::error!("Error stoping the `MediaCodec`: {e}");
            }
            AMediaCodec_delete(self.0.as_ptr());
        }
    }
}

impl MediaEngine {
    /// Create a new `MediaEngine`.
    pub fn create_by_name(name: &str) -> Result<MediaEngine, MediaStatus> {
        let name = CString::new(name).map_err(|_| MediaStatus::StringNulError)?;
        let ptr = unsafe { AMediaCodec_createCodecByName(name.as_ptr().cast()) };
        if let Some(decoder) = NonNull::new(ptr) {
            Ok(MediaEngine(decoder))
        } else {
            Err(MediaStatus::MediaCodecCreationFailed)
        }
    }

    /// Convert to an Android NDK [AMediaCodec] pointer.
    pub fn as_inner(&self) -> *mut AMediaCodec {
        self.0.as_ptr()
    }

    /// Initializes using the given format then start the `MediaCodec`.
    ///
    /// This is a combination of the configure and start steps.
    pub fn initialize(
        &mut self,
        format: &MediaFormat,
        window: Option<NativeWindow>,
        is_encoder: bool,
    ) -> Result<(), MediaStatus> {
        let surface = if let Some(window) = window {
            window.as_inner()
        } else {
            std::ptr::null_mut()
        };
        let flags = if is_encoder {
            AMEDIACODEC_CONFIGURE_FLAG_ENCODE as u32
        } else {
            0
        };
        unsafe {
            AMediaCodec_configure(
                self.as_inner(),
                format.as_inner(),
                surface,
                std::ptr::null_mut(),
                flags,
            )
            .success()?;
            AMediaCodec_start(self.as_inner()).success()
        }
    }

    /// Resets the output of the decoder to a new surface.
    pub fn set_output_surface(&self, window: &NativeWindow) -> Result<(), MediaStatus> {
        unsafe { AMediaCodec_setOutputSurface(self.as_inner(), window.as_inner()).success() }
    }

    /// Submits the codec specific data. Must be called before submitting frame data.
    pub fn submit_codec_config(&self, data: &[u8]) -> Result<(), MediaStatus> {
        let mut input_buffer = self.dequeue_input_buffer(MediaTimeout::INFINITE)?;
        let min_len = data.len().min(input_buffer.len());
        input_buffer[..min_len].copy_from_slice(&data[..min_len]);

        self.queue_input_buffer(
            input_buffer,
            min_len as c_ulong,
            0,
            AMEDIACODEC_BUFFER_FLAG_CODEC_CONFIG as u32,
        )
    }

    /// Get the next available input buffer. Returns `MediaStatus::NoAvailableBuffer` if no buffer
    /// is available after the timeout.
    #[inline(always)]
    pub fn dequeue_input_buffer(
        &self,
        timeout: MediaTimeout,
    ) -> Result<MediaInputBuffer, MediaStatus> {
        let index = unsafe { AMediaCodec_dequeueInputBuffer(self.as_inner(), timeout.0) };
        if index == -1 {
            return Err(MediaStatus::NoAvailableBuffer);
        }
        let index = index as c_ulong;

        let mut buf_size = 0;
        unsafe {
            let buf_ptr = AMediaCodec_getInputBuffer(self.as_inner(), index, &mut buf_size);
            if buf_ptr.is_null() {
                Err(MediaStatus::AllocationError)
            } else {
                let buffer = std::slice::from_raw_parts_mut(buf_ptr, buf_size as usize);
                Ok(MediaInputBuffer { index, buffer })
            }
        }
    }

    /// Send the specified buffer to the codec for processing.
    #[inline(always)]
    pub fn queue_input_buffer(
        &self,
        input_buffer: MediaInputBuffer,
        num_bytes: c_ulong,
        present_time_micros: u64,
        flags: u32,
    ) -> Result<(), MediaStatus> {
        unsafe {
            AMediaCodec_queueInputBuffer(
                self.as_inner(),
                input_buffer.index,
                0,
                num_bytes,
                present_time_micros,
                flags,
            )
            .success()
        }
    }

    /// Renders the decoder output to the surface.
    #[inline(always)]
    pub fn release_output_buffer(
        &self,
        timeout: MediaTimeout,
        render: bool,
    ) -> Result<(), MediaStatus> {
        const TRY_AGAIN_LATER: c_long = AMEDIACODEC_INFO_TRY_AGAIN_LATER as c_long;
        const OUTPUT_FORMAT_CHANGED: c_long = AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED as c_long;
        const OUTPUT_BUFFERS_CHANGED: c_long = AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED as c_long;

        let mut buffer_info = MaybeUninit::uninit();

        match unsafe {
            AMediaCodec_dequeueOutputBuffer(self.as_inner(), buffer_info.as_mut_ptr(), timeout.0)
        } {
            TRY_AGAIN_LATER => {
                // This should be unreachable since timeout is set to be infinite
                Err(MediaStatus::NoAvailableBuffer)
            }
            OUTPUT_FORMAT_CHANGED => {
                // ignoring format change assuming the underlying surface can handle it
                Ok(())
            }

            OUTPUT_BUFFERS_CHANGED => {
                // Deprecated in API level 21 and this is using 23 as minimum. This should be
                // unreachable.
                Ok(())
            }
            index => {
                // Proper index, use on `AMediaCodec_releaseOutputBuffer`
                unsafe {
                    AMediaCodec_releaseOutputBuffer(self.as_inner(), index as c_ulong, render)
                        .success()
                }
            }
        }
    }
}

/// Input to the `MediaEngine`.
pub struct MediaInputBuffer<'a> {
    index: c_ulong,
    buffer: &'a mut [u8],
}

impl<'a> Deref for MediaInputBuffer<'a> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.buffer
    }
}

impl<'a> DerefMut for MediaInputBuffer<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer
    }
}

/// Timeout value for `MediaEngine` methods.
#[derive(Debug, Clone, Copy)]
pub struct MediaTimeout(i64);

impl MediaTimeout {
    /// Signals to the operation to wait indefinitely.
    pub const INFINITE: MediaTimeout = MediaTimeout(-1);

    /// Create a new `MediaTimeout`. The timeout given must be less than or equal to `i64::MAX`
    /// microseconds (9223372036854775807).
    pub fn new(timeout: Duration) -> MediaTimeout {
        let timeout_micros = timeout.as_micros();
        assert!(timeout_micros <= i64::MAX as u128);
        MediaTimeout(timeout_micros as i64)
    }
}
