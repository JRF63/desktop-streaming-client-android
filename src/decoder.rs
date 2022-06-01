use crate::media_format::MediaFormat;
use ndk_sys::{
    AMediaCodec, AMediaCodecBufferInfo, AMediaCodec_configure, AMediaCodec_createDecoderByType,
    AMediaCodec_delete, AMediaCodec_dequeueInputBuffer, AMediaCodec_dequeueOutputBuffer,
    AMediaCodec_getInputBuffer, AMediaCodec_queueInputBuffer, AMediaCodec_releaseOutputBuffer,
    AMediaCodec_setOutputSurface, AMediaCodec_start, AMediaCodec_stop, ANativeWindow,
};
use std::{
    num::NonZeroI32,
    os::raw::{c_long, c_ulong},
    ptr::NonNull,
};

#[repr(transparent)]
pub(crate) struct MediaDecoder(NonNull<AMediaCodec>);

impl Drop for MediaDecoder {
    fn drop(&mut self) {
        unsafe {
            let _ignored_result = self.stop();
            AMediaCodec_delete(self.0.as_ptr());
        }
    }
}

impl MediaDecoder {
    pub(crate) fn create_from_format(
        format: &MediaFormat,
        raw_native_window: NonNull<ANativeWindow>,
    ) -> anyhow::Result<Self> {
        let mut decoder = {
            let ptr = unsafe { AMediaCodec_createDecoderByType(format.get_mime_type()) };
            if let Some(decoder) = NonNull::new(ptr) {
                MediaDecoder(decoder)
            } else {
                anyhow::bail!("`NonAMediaCodec_createDecoderByType` returned a null");
            }
        };

        decoder.configure(format, raw_native_window)?;
        decoder.start()?;
        Ok(decoder)
    }

    pub(crate) fn as_inner(&self) -> *mut AMediaCodec {
        self.0.as_ptr()
    }

    pub(crate) fn start(&self) -> Result<(), MediaStatus> {
        unsafe { AMediaCodec_start(self.as_inner()).success() }
    }

    pub(crate) fn stop(&self) -> Result<(), MediaStatus> {
        unsafe { AMediaCodec_stop(self.as_inner()).success() }
    }

    pub(crate) fn set_output_surface(
        &self,
        raw_native_window: NonNull<ANativeWindow>,
    ) -> Result<(), MediaStatus> {
        unsafe {
            AMediaCodec_setOutputSurface(self.as_inner(), raw_native_window.as_ptr()).success()
        }
    }

    pub(crate) fn try_decode(
        &self,
        data: &[u8],
        time: u64,
        end_of_stream: bool,
    ) -> anyhow::Result<bool> {
        match self.dequeue_input_buffer(0) {
            -1 => Ok(false),
            index => {
                let index = index as c_ulong;
                let buffer = self.get_input_buffer(index)?;

                let min_len = data.len().min(buffer.len());
                buffer[..min_len].copy_from_slice(&data[..min_len]);

                let flags = if end_of_stream {
                    ndk_sys::AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM as u32
                } else {
                    0
                };

                self.queue_input_buffer(index, 0, min_len as c_ulong, time, flags)?;
                Ok(true)
            }
        }
    }

    pub(crate) fn try_render(&self) -> anyhow::Result<bool> {
        const TRY_AGAIN_LATER: c_long = ndk_sys::AMEDIACODEC_INFO_TRY_AGAIN_LATER as c_long;
        const OUTPUT_FORMAT_CHANGED: c_long =
            ndk_sys::AMEDIACODEC_INFO_OUTPUT_FORMAT_CHANGED as c_long;
        const OUTPUT_BUFFERS_CHANGED: c_long =
            ndk_sys::AMEDIACODEC_INFO_OUTPUT_BUFFERS_CHANGED as c_long;

        let mut buffer_info = AMediaCodecBufferInfo {
            offset: 0,
            size: 0,
            presentationTimeUs: 0,
            flags: 0,
        };
        match self.dequeue_output_buffer(&mut buffer_info, 0) {
            TRY_AGAIN_LATER => Ok(false),
            // ignoring format change assuming the underlying surface can handle it
            OUTPUT_FORMAT_CHANGED => Ok(false),
            // deprecated in API level 21 and this is using 23 as minimum
            OUTPUT_BUFFERS_CHANGED => Ok(false),
            index => {
                self.release_output_buffer(index as c_ulong, true)?;
                Ok(true)
            }
        }
    }

    fn configure(
        &mut self,
        format: &MediaFormat,
        raw_native_window: NonNull<ANativeWindow>,
    ) -> Result<(), MediaStatus> {
        unsafe {
            AMediaCodec_configure(
                self.as_inner(),
                format.as_inner(),
                raw_native_window.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
            .success()
        }
    }

    fn dequeue_input_buffer(&self, timeout_us: i64) -> c_long {
        unsafe { AMediaCodec_dequeueInputBuffer(self.as_inner(), timeout_us) }
    }

    fn get_input_buffer(&self, index: c_ulong) -> anyhow::Result<&mut [u8]> {
        let mut buf_size = 0;
        unsafe {
            let buf_ptr = AMediaCodec_getInputBuffer(self.as_inner(), index, &mut buf_size);
            if buf_ptr.is_null() {
                anyhow::bail!("`AMediaCodec_getInputBuffer` returned a null");
            }
            Ok(std::slice::from_raw_parts_mut(buf_ptr, buf_size as usize))
        }
    }

    fn queue_input_buffer(
        &self,
        index: c_ulong,
        offset: i64,
        size: c_ulong,
        time: u64,
        flags: u32,
    ) -> Result<(), MediaStatus> {
        unsafe {
            AMediaCodec_queueInputBuffer(self.as_inner(), index, offset, size, time, flags)
                .success()
        }
    }

    fn dequeue_output_buffer(
        &self,
        buffer_info: &mut AMediaCodecBufferInfo,
        timeout_us: i64,
    ) -> c_long {
        unsafe { AMediaCodec_dequeueOutputBuffer(self.as_inner(), buffer_info, timeout_us) }
    }

    fn release_output_buffer(&self, index: c_ulong, render: bool) -> Result<(), MediaStatus> {
        unsafe { AMediaCodec_releaseOutputBuffer(self.as_inner(), index, render).success() }
    }
}

#[derive(Debug)]
pub(crate) struct MediaStatus(NonZeroI32);

impl MediaStatus {
    fn err_str(&self) -> &'static str {
        match self.0.get() {
            ndk_sys::media_status_t_AMEDIA_OK => "AMEDIA_OK",
            ndk_sys::media_status_t_AMEDIACODEC_ERROR_INSUFFICIENT_RESOURCE => {
                "AMEDIACODEC_ERROR_INSUFFICIENT_RESOURCE"
            }
            ndk_sys::media_status_t_AMEDIACODEC_ERROR_RECLAIMED => "AMEDIACODEC_ERROR_RECLAIMED",
            // AMEDIA_DRM_ERROR_BASE is the same as AMEDIA_ERROR_UNKNOWN
            ndk_sys::media_status_t_AMEDIA_ERROR_UNKNOWN => "AMEDIA_ERROR_UNKNOWN",
            ndk_sys::media_status_t_AMEDIA_ERROR_MALFORMED => "AMEDIA_ERROR_MALFORMED",
            ndk_sys::media_status_t_AMEDIA_ERROR_UNSUPPORTED => "AMEDIA_ERROR_UNSUPPORTED",
            ndk_sys::media_status_t_AMEDIA_ERROR_INVALID_OBJECT => "AMEDIA_ERROR_INVALID_OBJECT",
            ndk_sys::media_status_t_AMEDIA_ERROR_INVALID_PARAMETER => {
                "AMEDIA_ERROR_INVALID_PARAMETER"
            }
            ndk_sys::media_status_t_AMEDIA_ERROR_INVALID_OPERATION => {
                "AMEDIA_ERROR_INVALID_OPERATION"
            }
            ndk_sys::media_status_t_AMEDIA_ERROR_END_OF_STREAM => "AMEDIA_ERROR_END_OF_STREAM",
            ndk_sys::media_status_t_AMEDIA_ERROR_IO => "AMEDIA_ERROR_IO",
            ndk_sys::media_status_t_AMEDIA_ERROR_WOULD_BLOCK => "AMEDIA_ERROR_WOULD_BLOCK",
            ndk_sys::media_status_t_AMEDIA_DRM_ERROR_BASE => "AMEDIA_DRM_ERROR_BASE",
            ndk_sys::media_status_t_AMEDIA_DRM_NOT_PROVISIONED => "AMEDIA_DRM_NOT_PROVISIONED",
            ndk_sys::media_status_t_AMEDIA_DRM_RESOURCE_BUSY => "AMEDIA_DRM_RESOURCE_BUSY",
            ndk_sys::media_status_t_AMEDIA_DRM_DEVICE_REVOKED => "AMEDIA_DRM_DEVICE_REVOKED",
            ndk_sys::media_status_t_AMEDIA_DRM_SHORT_BUFFER => "AMEDIA_DRM_SHORT_BUFFER",
            ndk_sys::media_status_t_AMEDIA_DRM_SESSION_NOT_OPENED => {
                "AMEDIA_DRM_SESSION_NOT_OPENED"
            }
            ndk_sys::media_status_t_AMEDIA_DRM_TAMPER_DETECTED => "AMEDIA_DRM_TAMPER_DETECTED",
            ndk_sys::media_status_t_AMEDIA_DRM_VERIFY_FAILED => "AMEDIA_DRM_VERIFY_FAILED",
            ndk_sys::media_status_t_AMEDIA_DRM_NEED_KEY => "AMEDIA_DRM_NEED_KEY",
            ndk_sys::media_status_t_AMEDIA_DRM_LICENSE_EXPIRED => "AMEDIA_DRM_LICENSE_EXPIRED",
            ndk_sys::media_status_t_AMEDIA_IMGREADER_ERROR_BASE => "AMEDIA_IMGREADER_ERROR_BASE",
            ndk_sys::media_status_t_AMEDIA_IMGREADER_NO_BUFFER_AVAILABLE => {
                "AMEDIA_IMGREADER_NO_BUFFER_AVAILABLE"
            }
            ndk_sys::media_status_t_AMEDIA_IMGREADER_MAX_IMAGES_ACQUIRED => {
                "AMEDIA_IMGREADER_MAX_IMAGES_ACQUIRED"
            }
            ndk_sys::media_status_t_AMEDIA_IMGREADER_CANNOT_LOCK_IMAGE => {
                "AMEDIA_IMGREADER_CANNOT_LOCK_IMAGE"
            }
            ndk_sys::media_status_t_AMEDIA_IMGREADER_CANNOT_UNLOCK_IMAGE => {
                "AMEDIA_IMGREADER_CANNOT_UNLOCK_IMAGE"
            }
            ndk_sys::media_status_t_AMEDIA_IMGREADER_IMAGE_NOT_LOCKED => {
                "AMEDIA_IMGREADER_IMAGE_NOT_LOCKED"
            }
            _ => "MediaStatus unknown error",
        }
    }
}

impl std::fmt::Display for MediaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.err_str())
    }
}

impl std::error::Error for MediaStatus {}

trait AsMediaStatus {
    fn success(self) -> Result<(), MediaStatus>;
}

impl AsMediaStatus for i32 {
    fn success(self) -> Result<(), MediaStatus> {
        match NonZeroI32::new(self) {
            Some(nonzero) => Err(MediaStatus(nonzero)),
            None => Ok(()), // AMEDIA_OK
        }
    }
}
