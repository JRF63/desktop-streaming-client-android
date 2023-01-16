#![allow(dead_code)] // Suppress warnings for now

use super::{
    MediaFormat, MediaFormatData, MediaFormatMimeType, H264_MIME_TYPE, HEVC_MIME_TYPE,
    MEDIAFORMAT_KEY_CSD_0, MEDIAFORMAT_KEY_CSD_1,
};
use std::ffi::CStr;

#[derive(Clone, Copy)]
pub enum VideoType {
    H264,
    Hevc,
}

impl MediaFormatMimeType for VideoType {
    fn mime_type(&self) -> &CStr {
        let s = match self {
            VideoType::H264 => H264_MIME_TYPE,
            VideoType::Hevc => HEVC_MIME_TYPE,
        };
        unsafe { CStr::from_ptr(s.as_ptr().cast()) }
    }
}

/// Find the starting positions of the [0x0, 0x0, 0x0, 0x1] marker.
fn nal_boundaries(data: &[u8]) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(3);

    let mut zeroes = 0;
    for (i, &byte) in data.iter().enumerate() {
        match byte {
            0 => zeroes += 1,
            1 => {
                if zeroes >= 2 {
                    boundaries.push(i - zeroes);
                }
                zeroes = 0;
            }
            _ => zeroes = 0,
        }
    }
    boundaries
}

/// Used for manually setting H264 specific data. `AMediaFormat_setBuffer` with
/// `AMEDIAFORMAT_KEY_CSD_AVC` (API level >=29) can be used to pass the CSD buffer as a whole.
pub struct H264Csd<'a> {
    csd0: &'a [u8],
    csd1: &'a [u8],
}

impl<'a> MediaFormatData for H264Csd<'a> {
    fn add_to_media_format(&self, media_format: &mut MediaFormat) {
        media_format.set_buffer(MEDIAFORMAT_KEY_CSD_0.as_ptr().cast(), self.csd0);
        media_format.set_buffer(MEDIAFORMAT_KEY_CSD_1.as_ptr().cast(), self.csd1);
    }
}

impl<'a> H264Csd<'a> {
    /// Create a `H264Csd` from a byte buffer. This involves finding where the SPS and PPS are in
    /// the buffer. Returns `None` if they cannot be found.
    pub fn from_slice(data: &'a [u8]) -> Option<Self> {
        const SPS_NAL_UNIT_TYPE: u8 = 7;
        const PPS_NAL_UNIT_TYPE: u8 = 8;
        const NAL_UNIT_TYPE_MASK: u8 = 0b11111;

        let mut csd0 = None;
        let mut csd1 = None;

        let mut boundaries = nal_boundaries(data);
        boundaries.push(data.len());

        for window in boundaries.windows(2) {
            if let &[i, j] = window {
                let nal = data.get(i..j)?;
                match nal.get(4)? & NAL_UNIT_TYPE_MASK {
                    SPS_NAL_UNIT_TYPE => csd0 = Some(nal),
                    PPS_NAL_UNIT_TYPE => csd1 = Some(nal),
                    _ => (),
                }

                if let (Some(csd0), Some(csd1)) = (csd0, csd1) {
                    return Some(H264Csd { csd0, csd1 });
                }
            }
        }

        None
    }
}

/// Used for manually setting HEVC specific data. `AMediaFormat_setBuffer` with
/// `AMEDIAFORMAT_KEY_CSD_HEVC` (API level >=29) can be used instead.
pub struct HevcCsd<'a> {
    csd0: &'a [u8],
}

impl<'a> MediaFormatData for HevcCsd<'a> {
    fn add_to_media_format(&self, media_format: &mut MediaFormat) {
        media_format.set_buffer(MEDIAFORMAT_KEY_CSD_0.as_ptr().cast(), self.csd0);
    }
}

impl<'a> HevcCsd<'a> {
    /// Create a `HevcCsd` from a byte buffer. This needs to check for the presence of VPS, SPS and
    /// PPS NALs. Returns `None` if it fails.
    pub fn from_slice(_data: &'a [u8]) -> Option<Self> {
        todo!()
    }
}
