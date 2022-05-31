use crate::media_format::MediaFormat;
use ndk_sys::{AMediaCodec, AMediaCodec_createDecoderByType, AMediaCodec_configure, AMediaCodec_delete, ANativeWindow, media_status_t_AMEDIA_OK};
use std::ptr::NonNull;

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
        let decoder = {
            let ptr = unsafe { AMediaCodec_createDecoderByType(format.get_mime_type()) };
            if let Some(decoder) = NonNull::new(ptr) {
                MediaDecoder(decoder)
            } else {
                anyhow::bail!("`NonAMediaCodec_createDecoderByType` returned a null");
            }
        };

        unsafe {
            let status = AMediaCodec_configure(
                decoder.as_inner(),
                format.as_inner(),
                raw_native_window.as_ptr(),
                std::ptr::null_mut(),
                0
            );
            if status != media_status_t_AMEDIA_OK {
                anyhow::bail!("`AMediaCodec_configure` error: {}", status);
            }
        }
        Ok(decoder)
    }

    pub(crate) fn as_inner(&self) -> *mut AMediaCodec {
        self.0.as_ptr()
    }
}
