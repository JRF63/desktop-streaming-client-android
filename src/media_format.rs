use ndk_sys::{
    AMediaFormat, AMediaFormat_delete, AMediaFormat_getString, AMediaFormat_new,
    AMediaFormat_setBuffer, AMediaFormat_setInt32, AMediaFormat_setString,
    AMEDIAFORMAT_KEY_FRAME_RATE, AMEDIAFORMAT_KEY_HEIGHT, AMEDIAFORMAT_KEY_MIME,
    AMEDIAFORMAT_KEY_WIDTH,
};
use std::ptr::NonNull;

#[repr(transparent)]
pub(crate) struct MediaFormat(NonNull<AMediaFormat>);

impl Drop for MediaFormat {
    fn drop(&mut self) {
        unsafe {
            AMediaFormat_delete(self.0.as_ptr());
        }
    }
}

impl MediaFormat {
    pub(crate) fn create_video_format(
        video_type: VideoType,
        width: i32,
        height: i32,
        frame_rate: i32,
        csd: &[u8],
    ) -> anyhow::Result<Self> {
        unsafe {
            if let Some(media_format) = NonNull::new(AMediaFormat_new()) {
                let mut media_format = MediaFormat(media_format);

                media_format.set_video_type(video_type);
                media_format.set_width(width);
                media_format.set_height(height);
                media_format.set_frame_rate(frame_rate);

                match video_type {
                    VideoType::H264 => {
                        let boundaries = nal_boundaries(csd);

                        if boundaries.len() == 2 {
                            if let (Some(first), Some(second)) = (
                                csd.get(boundaries[0]..boundaries[1]),
                                csd.get(boundaries[1]..),
                            ) {
                                if let (Some(csd0), Some(csd1)) = (
                                    H264Csd::get_nal_unit_type(first),
                                    H264Csd::get_nal_unit_type(second),
                                ) {
                                    if csd0 != csd1 {
                                        media_format.set_h264_csd(csd0, first);
                                        media_format.set_h264_csd(csd1, second);
                                        crate::info!("SUccess");
                                        return Ok(media_format);
                                    }
                                }
                            }
                        }

                        anyhow::bail!("Invalid SPS/PPS data")
                    }
                    VideoType::Hevc => todo!(),
                }
            } else {
                anyhow::bail!("AMediaFormat_new returned a null");
            }
        }
    }

    pub(crate) fn as_inner(&self) -> *mut AMediaFormat {
        self.0.as_ptr()
    }

    fn set_video_type(&mut self, video_type: VideoType) {
        unsafe {
            AMediaFormat_setString(
                self.as_inner(),
                AMEDIAFORMAT_KEY_MIME,
                video_type.as_cstr_ptr(),
            );
        }
    }

    fn set_width(&mut self, width: i32) {
        unsafe {
            AMediaFormat_setInt32(self.as_inner(), AMEDIAFORMAT_KEY_WIDTH, width);
        }
    }

    fn set_height(&mut self, height: i32) {
        unsafe {
            AMediaFormat_setInt32(self.as_inner(), AMEDIAFORMAT_KEY_HEIGHT, height);
        }
    }

    fn set_frame_rate(&mut self, frame_rate: i32) {
        unsafe {
            AMediaFormat_setInt32(self.as_inner(), AMEDIAFORMAT_KEY_FRAME_RATE, frame_rate);
        }
    }

    fn set_h264_csd(&mut self, csd: H264Csd, data: &[u8]) {
        let name = match csd {
            H264Csd::SPS => "csd-0\0",
            H264Csd::PPS => "csd-1\0",
        };
        unsafe {
            AMediaFormat_setBuffer(
                self.as_inner(),
                name.as_ptr().cast(),
                data.as_ptr().cast(),
                data.len() as u64,
            );
        }
    }

    pub(crate) fn get_mime_type(&self) -> *const i8 {
        unsafe {
            let mut cstr: *const i8 = std::ptr::null();
            AMediaFormat_getString(self.as_inner(), AMEDIAFORMAT_KEY_MIME, &mut cstr);
            cstr
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum VideoType {
    H264,
    Hevc,
}

impl VideoType {
    pub(crate) fn as_cstr_ptr(&self) -> *const std::os::raw::c_char {
        self.mime_cstr().as_ptr().cast()
    }

    fn mime_cstr(&self) -> &'static str {
        match self {
            VideoType::H264 => "video/avc\0",
            VideoType::Hevc => "video/hevc\0",
        }
    }
}

fn nal_boundaries(data: &[u8]) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(3);

    let mut zeroes = 0;
    for (i, &byte) in data.iter().enumerate() {
        match byte {
            0 => zeroes += 1,
            1 => {
                if zeroes == 3 {
                    boundaries.push(i - 3);
                }
                zeroes = 0;
            }
            _ => zeroes = 0,
        }
    }
    boundaries
}

#[derive(Clone, Copy, PartialEq)]
enum H264Csd {
    SPS = 7,
    PPS = 8,
}

impl H264Csd {
    const SPS_NAL_UNIT_TYPE: u8 = 7;
    const PPS_NAL_UNIT_TYPE: u8 = 8;
    const NAL_UNIT_TYPE_MASK: u8 = 0b11111;

    fn get_nal_unit_type(data: &[u8]) -> Option<H264Csd> {
        let v = data.get(4)?;
        match v & H264Csd::NAL_UNIT_TYPE_MASK {
            H264Csd::SPS_NAL_UNIT_TYPE => Some(H264Csd::SPS),
            H264Csd::PPS_NAL_UNIT_TYPE => Some(H264Csd::PPS),
            _ => None,
        }
    }
}
