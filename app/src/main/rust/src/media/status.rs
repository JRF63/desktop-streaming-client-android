use ndk_sys::media_status_t;

// Error returned by the media codec API.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum MediaStatus {
    Sys(NonZeroSysMediaStatus),
    AllocationError,
    StringNulError,
    MediaCodecCreationFailed,
    NoAvailableBuffer,
}

// Required for `std::error::Error`. Format using `std::fmt::Debug`.
impl std::fmt::Display for MediaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for MediaStatus {}

/// Helper trait for ease of error handling of `ndk_sys::media_status_t`.
pub trait AsMediaStatus: private::Sealed {
    /// Return `Ok(())` if `AMEDIA_OK` else return an error.
    fn success(self) -> Result<(), MediaStatus>;
}

impl AsMediaStatus for media_status_t {
    fn success(self) -> Result<(), MediaStatus> {
        match NonZeroSysMediaStatus::try_from(self) {
            Ok(n) => Err(MediaStatus::Sys(n)),
            Err(_) => Ok(()),
        }
    }
}

mod private {
    pub trait Sealed {}

    impl Sealed for ndk_sys::media_status_t {}
}

/// `ndk_sys::media_status_t` but excluding `AMEDIA_OK`
///
/// Also excludes `AMEDIA_ERROR_BASE` because it is a duplicate of `AMEDIA_ERROR_UNKNOWN`.
#[allow(non_camel_case_types)]
#[non_exhaustive]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
#[repr(i32)]
pub enum NonZeroSysMediaStatus {
    // AMEDIA_OK = 0,
    AMEDIACODEC_ERROR_INSUFFICIENT_RESOURCE = 1100,
    AMEDIACODEC_ERROR_RECLAIMED = 1101,
    // AMEDIA_ERROR_BASE = -10000,
    AMEDIA_ERROR_UNKNOWN = -10000,
    AMEDIA_ERROR_MALFORMED = -10001,
    AMEDIA_ERROR_UNSUPPORTED = -10002,
    AMEDIA_ERROR_INVALID_OBJECT = -10003,
    AMEDIA_ERROR_INVALID_PARAMETER = -10004,
    AMEDIA_ERROR_INVALID_OPERATION = -10005,
    AMEDIA_ERROR_END_OF_STREAM = -10006,
    AMEDIA_ERROR_IO = -10007,
    AMEDIA_ERROR_WOULD_BLOCK = -10008,
    AMEDIA_DRM_ERROR_BASE = -20000,
    AMEDIA_DRM_NOT_PROVISIONED = -20001,
    AMEDIA_DRM_RESOURCE_BUSY = -20002,
    AMEDIA_DRM_DEVICE_REVOKED = -20003,
    AMEDIA_DRM_SHORT_BUFFER = -20004,
    AMEDIA_DRM_SESSION_NOT_OPENED = -20005,
    AMEDIA_DRM_TAMPER_DETECTED = -20006,
    AMEDIA_DRM_VERIFY_FAILED = -20007,
    AMEDIA_DRM_NEED_KEY = -20008,
    AMEDIA_DRM_LICENSE_EXPIRED = -20009,
    AMEDIA_IMGREADER_ERROR_BASE = -30000,
    AMEDIA_IMGREADER_NO_BUFFER_AVAILABLE = -30001,
    AMEDIA_IMGREADER_MAX_IMAGES_ACQUIRED = -30002,
    AMEDIA_IMGREADER_CANNOT_LOCK_IMAGE = -30003,
    AMEDIA_IMGREADER_CANNOT_UNLOCK_IMAGE = -30004,
    AMEDIA_IMGREADER_IMAGE_NOT_LOCKED = -30005,
}

impl TryFrom<media_status_t> for NonZeroSysMediaStatus {
    type Error = ();

    // Skip formating to have the cases be one line each
    #[rustfmt::skip]
    fn try_from(value: media_status_t) -> Result<Self, Self::Error> {
        match value {
            media_status_t::AMEDIA_OK => Err(()),
            media_status_t::AMEDIACODEC_ERROR_INSUFFICIENT_RESOURCE => Ok(NonZeroSysMediaStatus::AMEDIACODEC_ERROR_INSUFFICIENT_RESOURCE),
            media_status_t::AMEDIACODEC_ERROR_RECLAIMED => Ok(NonZeroSysMediaStatus::AMEDIACODEC_ERROR_RECLAIMED),
            media_status_t::AMEDIA_ERROR_UNKNOWN => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_UNKNOWN),
            media_status_t::AMEDIA_ERROR_MALFORMED => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_MALFORMED),
            media_status_t::AMEDIA_ERROR_UNSUPPORTED => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_UNSUPPORTED),
            media_status_t::AMEDIA_ERROR_INVALID_OBJECT => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_INVALID_OBJECT),
            media_status_t::AMEDIA_ERROR_INVALID_PARAMETER => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_INVALID_PARAMETER),
            media_status_t::AMEDIA_ERROR_INVALID_OPERATION => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_INVALID_OPERATION),
            media_status_t::AMEDIA_ERROR_END_OF_STREAM => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_END_OF_STREAM),
            media_status_t::AMEDIA_ERROR_IO => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_IO),
            media_status_t::AMEDIA_ERROR_WOULD_BLOCK => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_WOULD_BLOCK),
            media_status_t::AMEDIA_DRM_ERROR_BASE => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_ERROR_BASE),
            media_status_t::AMEDIA_DRM_NOT_PROVISIONED => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_NOT_PROVISIONED),
            media_status_t::AMEDIA_DRM_RESOURCE_BUSY => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_RESOURCE_BUSY),
            media_status_t::AMEDIA_DRM_DEVICE_REVOKED => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_DEVICE_REVOKED),
            media_status_t::AMEDIA_DRM_SHORT_BUFFER => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_SHORT_BUFFER),
            media_status_t::AMEDIA_DRM_SESSION_NOT_OPENED => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_SESSION_NOT_OPENED),
            media_status_t::AMEDIA_DRM_TAMPER_DETECTED => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_TAMPER_DETECTED),
            media_status_t::AMEDIA_DRM_VERIFY_FAILED => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_VERIFY_FAILED),
            media_status_t::AMEDIA_DRM_NEED_KEY => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_NEED_KEY),
            media_status_t::AMEDIA_DRM_LICENSE_EXPIRED => Ok(NonZeroSysMediaStatus::AMEDIA_DRM_LICENSE_EXPIRED),
            media_status_t::AMEDIA_IMGREADER_ERROR_BASE => Ok(NonZeroSysMediaStatus::AMEDIA_IMGREADER_ERROR_BASE),
            media_status_t::AMEDIA_IMGREADER_NO_BUFFER_AVAILABLE => Ok(NonZeroSysMediaStatus::AMEDIA_IMGREADER_NO_BUFFER_AVAILABLE),
            media_status_t::AMEDIA_IMGREADER_MAX_IMAGES_ACQUIRED => Ok(NonZeroSysMediaStatus::AMEDIA_IMGREADER_MAX_IMAGES_ACQUIRED),
            media_status_t::AMEDIA_IMGREADER_CANNOT_LOCK_IMAGE => Ok(NonZeroSysMediaStatus::AMEDIA_IMGREADER_CANNOT_LOCK_IMAGE),
            media_status_t::AMEDIA_IMGREADER_CANNOT_UNLOCK_IMAGE => Ok(NonZeroSysMediaStatus::AMEDIA_IMGREADER_CANNOT_UNLOCK_IMAGE),
            media_status_t::AMEDIA_IMGREADER_IMAGE_NOT_LOCKED => Ok(NonZeroSysMediaStatus::AMEDIA_IMGREADER_IMAGE_NOT_LOCKED),
            _ => Ok(NonZeroSysMediaStatus::AMEDIA_ERROR_UNKNOWN),
        }
    }
}
