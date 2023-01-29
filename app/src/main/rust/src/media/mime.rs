use std::{ffi::CStr, str::FromStr};

/// Abstraction of a MIME type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MimeType {
    AudioPcma, //  "audio/g711-alaw", "audio/PCMA"
    AudioPcmu, // "audio/g711-mlaw", "audio/PCMU"
    AudioOpus, // "audio/opus", "audio/opus"
    VideoAv1,  // "video/av01", "video/AV1"
    VideoH264, // "video/avc", "video/H264"
    VideoH265, // "video/hevc", "video/H265"
    VideoVp8,  // "video/x-vnd.on2.vp8", "video/VP8"
}

impl MimeType {
    fn to_nul_terminated_rust_str(self) -> &'static str {
        match self {
            MimeType::AudioPcma => "audio/g711-alaw\0",
            MimeType::AudioPcmu => "audio/g711-mlaw\0",
            MimeType::AudioOpus => "audio/opus\0",
            MimeType::VideoAv1 => "video/av01\0",
            MimeType::VideoH264 => "video/avc\0",
            MimeType::VideoH265 => "video/hevc\0",
            MimeType::VideoVp8 => "video/x-vnd.on2.vp8\0",
        }
    }

    /// Convert `MimeType` to the nul terminated string that Android understands.
    pub fn to_android_cstr(self) -> &'static CStr {
        let s = self.to_nul_terminated_rust_str();
        unsafe { CStr::from_ptr(s.as_ptr().cast()) }
    }

    /// Convert `MimeType` to a `str` that Android understands.
    pub fn to_android_str(self) -> &'static str {
        let s = self.to_nul_terminated_rust_str();
        s.trim_end_matches('\0')
    }

    /// Convert `MimeType` to SDP MIME type.
    pub fn to_sdp_str(self) -> &'static str {
        match self {
            MimeType::AudioPcma => "audio/PCMA",
            MimeType::AudioPcmu => "audio/PCMU",
            MimeType::AudioOpus => "audio/opus",
            MimeType::VideoAv1 => "video/AV1",
            MimeType::VideoH264 => "video/H264",
            MimeType::VideoH265 => "video/H265",
            MimeType::VideoVp8 => "video/VP8",
        }
    }
}

impl FromStr for MimeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        macro_rules! impl_from_str {
            ($s1:expr, $s2:expr, $out:tt) => {
                if s.eq_ignore_ascii_case($s1) || s.eq_ignore_ascii_case($s2) {
                    return Ok(MimeType::$out);
                }
            };
        }

        impl_from_str!("audio/g711-alaw", "audio/PCMA", AudioPcma);
        impl_from_str!("audio/g711-mlaw", "audio/PCMU", AudioPcmu);
        impl_from_str!("audio/opus", "audio/opus", AudioOpus);
        impl_from_str!("video/av01", "video/AV1", VideoAv1);
        impl_from_str!("video/avc", "video/H264", VideoH264);
        impl_from_str!("video/hevc", "video/H265", VideoH265);
        impl_from_str!("video/x-vnd.on2.vp8", "video/VP8", VideoVp8);
        Err(())
    }
}
