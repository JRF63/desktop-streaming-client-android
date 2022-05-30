use ndk_sys::{AMediaCodec, AMediaCodec_createDecoderByType};
use std::ptr::NonNull;

#[repr(transparent)]
pub(crate) struct Decoder(NonNull<AMediaCodec>);

impl Decoder {
    pub(crate) fn create_from_type(decoder_type: DecoderType) -> anyhow::Result<Self> {
        unsafe {
            let ptr = AMediaCodec_createDecoderByType(decoder_type.as_c_ptr());
            if let Some(decoder) = NonNull::new(ptr) {
                Ok(Decoder(decoder))
            } else {
                anyhow::bail!("`NonAMediaCodec_createDecoderByType` returned a null");
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum DecoderType {
    H264,
    HEVC,
}

impl DecoderType {
    pub(crate) fn as_c_ptr(&self) -> *const std::os::raw::c_char {
        self.mime_cstr().as_ptr().cast()
    }

    fn mime_cstr(&self) -> &'static str {
        match self {
            DecoderType::H264 => "video/avc\0",
            DecoderType::HEVC => "video/hevc\0",
        }
    }
}