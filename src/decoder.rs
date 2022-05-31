use crate::media_format::MediaFormat;
use ndk_sys::{
    AMediaCodec, AMediaCodec_configure, AMediaCodec_createDecoderByType, AMediaCodec_delete,
    AMediaCodec_dequeueInputBuffer, AMediaCodec_dequeueOutputBuffer, AMediaCodec_getInputBuffer,
    AMediaCodec_getOutputBuffer, AMediaCodec_queueInputBuffer, AMediaCodec_releaseOutputBuffer,
    AMediaCodec_start, AMediaCodec_stop, ANativeWindow, AMediaCodecBufferInfo
};
use std::{num::NonZeroI32, os::raw::c_long, ptr::NonNull};

#[repr(transparent)]
pub(crate) struct MediaDecoder(NonNull<AMediaCodec>);

impl Drop for MediaDecoder {
    fn drop(&mut self) {
        unsafe {
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
        Ok(decoder)
    }

    pub(crate) fn as_inner(&self) -> *mut AMediaCodec {
        self.0.as_ptr()
    }

    pub(crate) fn start(&self) -> Result<(), MediaStatus> {
        unsafe {
            AMediaCodec_start(self.as_inner()).success()?;
        }
        Ok(())
    }

    pub(crate) fn stop(&self) -> Result<(), MediaStatus> {
        unsafe {
            AMediaCodec_stop(self.as_inner()).success()?;
        }
        Ok(())
    }

    pub(crate) fn try_queue_input(
        &self,
        data: &[u8],
        time: u64,
        end_of_stream: bool,
    ) -> anyhow::Result<bool> {
        unsafe {
            match AMediaCodec_dequeueInputBuffer(self.as_inner(), 0) {
                -1 => Ok(false),
                index => {
                    let index = index as _;
                    let mut buf_size = 0;
                    let buf_ptr = AMediaCodec_getInputBuffer(self.as_inner(), index, &mut buf_size);
                    if buf_ptr.is_null() {
                        anyhow::bail!("`AMediaCodec_getInputBuffer` returned a null");
                    }

                    // crate::info!("input buf size: {}", buf_size);
                    let buffer = std::slice::from_raw_parts_mut(buf_ptr, buf_size as usize);

                    let min_len = data.len().min(buffer.len());
                    buffer[..min_len].copy_from_slice(&data[..min_len]);

                    let flags = if end_of_stream {
                        ndk_sys::AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM as _
                    } else {
                        0
                    };

                    AMediaCodec_queueInputBuffer(
                        self.as_inner(),
                        index,
                        0,
                        min_len as _,
                        time,
                        flags,
                    )
                    .success()?;
                    Ok(true)
                }
            }
        }
    }

    pub(crate) fn try_get_output(&self) -> anyhow::Result<bool> {
        unsafe {
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
            match AMediaCodec_dequeueOutputBuffer(self.as_inner(), &mut buffer_info, 0) {
                TRY_AGAIN_LATER => Ok(false),
                OUTPUT_FORMAT_CHANGED => Ok(false), // anyhow::bail!("Output format changed"),
                OUTPUT_BUFFERS_CHANGED => Ok(false), // anyhow::bail!("Output buffers changed"),
                index => {
                    let index = index as _;
                    let mut buf_size = 0;
                    let buf_ptr = AMediaCodec_getOutputBuffer(self.as_inner(), index, &mut buf_size);
                    if buf_ptr.is_null() {
                        anyhow::bail!("`AMediaCodec_getOutputBuffer` returned a null");
                    }

                    AMediaCodec_releaseOutputBuffer(self.as_inner(), index, true).success()?;
                    Ok(true)
                }
            }
        }
    }

    fn configure(
        &mut self,
        format: &MediaFormat,
        raw_native_window: NonNull<ANativeWindow>,
    ) -> anyhow::Result<()> {
        unsafe {
            AMediaCodec_configure(
                self.as_inner(),
                format.as_inner(),
                raw_native_window.as_ptr(),
                std::ptr::null_mut(),
                0,
            )
            .success()?;
        }
        Ok(())
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
