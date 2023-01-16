use super::{
    format::{MediaFormat, MediaFormatMimeType},
    status::{AsMediaStatus, MediaStatus},
};
use crate::window::NativeWindow;
use ndk_sys::{
    AMediaCodec, AMediaCodecBufferInfo, AMediaCodec_configure, AMediaCodec_createDecoderByType,
    AMediaCodec_delete, AMediaCodec_dequeueInputBuffer, AMediaCodec_dequeueOutputBuffer,
    AMediaCodec_getInputBuffer, AMediaCodec_queueInputBuffer, AMediaCodec_releaseOutputBuffer,
    AMediaCodec_setOutputSurface, AMediaCodec_start, AMediaCodec_stop,
    AMEDIACODEC_BUFFER_FLAG_CODEC_CONFIG, AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM,
    AMEDIACODEC_CONFIGURE_FLAG_ENCODE, AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED,
    AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED, AMEDIACODEC_INFO_TRY_AGAIN_LATER,
};
use std::{
    ffi::{c_long, c_ulong},
    mem::MaybeUninit,
    ptr::NonNull,
};

/// Encapsulates a encoder/decoder.
#[repr(transparent)]
pub struct MediaCodec(NonNull<AMediaCodec>);

// FIXME: Is this safe?
unsafe impl Send for MediaCodec {}

impl Drop for MediaCodec {
    fn drop(&mut self) {
        unsafe {
            if let Err(e) = self.signal_end_of_stream() {
                crate::error!("Error signaling end of stream: {e}");
            }
            if let Err(e) = AMediaCodec_stop(self.as_inner()).success() {
                crate::error!("Error stoping the `MediaCodec`: {e}");
            }
            AMediaCodec_delete(self.0.as_ptr());
        }
    }
}

impl MediaCodec {
    /// Create a decoder.
    pub fn new_decoder<T>(kind: T) -> Result<MediaCodec, MediaStatus>
    where
        T: MediaFormatMimeType,
    {
        let ptr = unsafe { AMediaCodec_createDecoderByType(kind.mime_type().as_ptr()) };
        if let Some(decoder) = NonNull::new(ptr) {
            Ok(MediaCodec(decoder))
        } else {
            Err(MediaStatus::NoDecoderForFormat)
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

    /// Submits the codec specific data. Must be called before `MediaCodec::decode`.
    pub fn submit_codec_config<F>(&self, func: F) -> Result<(), MediaStatus>
    where
        F: FnMut(&mut [u8]) -> (usize, u64),
    {
        self.decode_inner(func, AMEDIACODEC_BUFFER_FLAG_CODEC_CONFIG as u32)
    }

    /// Decodes the given data.
    pub fn decode<F>(&self, func: F) -> Result<(), MediaStatus>
    where
        F: FnMut(&mut [u8]) -> (usize, u64),
    {
        self.decode_inner(func, 0)
    }

    pub fn signal_end_of_stream(&self) -> Result<(), MediaStatus> {
        self.decode_inner(|_| (0, 0), AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM as u32)
    }

    #[inline(always)]
    fn decode_inner<F>(&self, mut func: F, flags: u32) -> Result<(), MediaStatus>
    where
        F: FnMut(&mut [u8]) -> (usize, u64),
    {
        let index = self.dequeue_input_buffer(-1)?;
        let buffer = self.get_input_buffer(index)?;
        let (num_bytes, present_time_micros) = func(buffer);
        self.queue_input_buffer(
            index,
            0,
            num_bytes as c_ulong,
            present_time_micros,
            flags as u32,
        )
    }

    /// Get the index of the next available input buffer. Returns `MediaStatus::NoAvailableBuffer`
    /// if no buffer is available after the timeout.
    fn dequeue_input_buffer(&self, timeout_micros: i64) -> Result<c_ulong, MediaStatus> {
        let index = unsafe { AMediaCodec_dequeueInputBuffer(self.as_inner(), timeout_micros) };
        match index {
            -1 => Err(MediaStatus::NoAvailableBuffer),
            index => Ok(index as c_ulong),
        }
    }

    /// Get an input buffer.
    fn get_input_buffer(&self, index: c_ulong) -> Result<&mut [u8], MediaStatus> {
        let mut buf_size = 0;
        unsafe {
            let buf_ptr = AMediaCodec_getInputBuffer(self.as_inner(), index, &mut buf_size);
            if buf_ptr.is_null() {
                Err(MediaStatus::AllocationError)
            } else {
                Ok(std::slice::from_raw_parts_mut(buf_ptr, buf_size as usize))
            }
        }
    }

    /// Send the specified buffer to the codec for processing.
    fn queue_input_buffer(
        &self,
        index: c_ulong,
        offset: i64,
        num_bytes: c_ulong,
        present_time_micros: u64,
        flags: u32,
    ) -> Result<(), MediaStatus> {
        unsafe {
            AMediaCodec_queueInputBuffer(
                self.as_inner(),
                index,
                offset,
                num_bytes,
                present_time_micros,
                flags,
            )
            .success()
        }
    }

    /// Renders the decoder output to the surface.
    pub fn render_output(&self) -> Result<(), MediaStatus> {
        const TRY_AGAIN_LATER: c_long = AMEDIACODEC_INFO_TRY_AGAIN_LATER as c_long;
        const OUTPUT_FORMAT_CHANGED: c_long = AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED as c_long;
        const OUTPUT_BUFFERS_CHANGED: c_long = AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED as c_long;

        // Leave uninit because this is unused.
        let mut buffer_info = MaybeUninit::uninit();

        match self.dequeue_output_buffer(buffer_info.as_mut_ptr(), -1) {
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
                self.release_output_buffer(index as c_ulong, true)?;
                Ok(())
            }
        }
    }

    /// Get the index of the next available buffer of processed data.
    fn dequeue_output_buffer(
        &self,
        buffer_info: *mut AMediaCodecBufferInfo,
        timeout_micros: i64,
    ) -> c_long {
        unsafe { AMediaCodec_dequeueOutputBuffer(self.as_inner(), buffer_info, timeout_micros) }
    }

    /// Return the buffer to the codec.
    fn release_output_buffer(&self, index: c_ulong, render: bool) -> Result<(), MediaStatus> {
        unsafe { AMediaCodec_releaseOutputBuffer(self.as_inner(), index, render).success() }
    }
}
