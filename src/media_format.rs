use ndk_sys::{
    AMediaFormat, AMediaFormat_delete, AMediaFormat_new, AMediaFormat_setInt32,
    AMediaFormat_setString, AMEDIAFORMAT_KEY_FRAME_RATE, AMEDIAFORMAT_KEY_HEIGHT,
    AMEDIAFORMAT_KEY_MIME, AMEDIAFORMAT_KEY_WIDTH,
};
use std::ptr::NonNull;

pub fn media_format_from_sample(sample: &[u8]) {
    use crate::{info, error};
    use ndk_sys::{AMediaExtractor_delete, AMediaExtractor_new, AMediaExtractor_setDataSourceCustom, AMediaExtractor_getTrackFormat};
    use ndk_sys::{AMediaDataSource_new, AMediaDataSource_close,
         AMediaDataSource_setReadAt,
         AMediaDataSource_setGetSize,
         AMediaDataSource_setUserdata,
         AMediaDataSource_setClose,
    };

    use std::ffi::c_void;
    use std::io::{Cursor, SeekFrom};
    use std::io::prelude::*;

    let mut cursor = Cursor::new(sample);

    unsafe extern "C" fn read_at(userdata: *mut c_void, offset: i64, buffer: *mut c_void, size: u64) -> i64 {
        let cursor: &mut Cursor<&[u8]> = &mut *userdata.cast();
        if cursor.get_ref().len() <= offset as usize {
            info!("hardcoded return");
            return -1;
        }
        if let Ok(_) = cursor.seek(SeekFrom::Start(offset as _)) {
            let dest = std::slice::from_raw_parts_mut(buffer as *mut u8, size as usize);
            if let Ok(bytes) = cursor.read(dest) {
                if bytes != 0 {
                    info!("read_at: {}, size: {}, result: {}", offset, size, bytes);
                    return bytes as i64;
                }
                info!("read_at: {}, size: {}, result: {}", offset, size, bytes);
                return bytes as i64;
            }
            // let _ = cursor.rewind();
            info!("read_at: {}, size: {}, result: {}", offset, size, 0);
            0
        } else {
            info!("Seek end - read_at: {}, size: {}, result: {}", offset, size, 0);
            0
        }
    }

    unsafe extern "C" fn get_size(userdata: *mut c_void) -> i64 {
        info!("get_size");
        let cursor: &mut Cursor<&[u8]> = &mut *userdata.cast();
        cursor.get_ref().len() as i64
    }

    unsafe extern "C" fn close(_userdata: *mut c_void) {

    }

    unsafe {
        let data_source = AMediaDataSource_new();
        AMediaDataSource_setUserdata(data_source, (&mut cursor as *mut Cursor<&[u8]>) as *mut c_void);
        AMediaDataSource_setReadAt(data_source, Some(read_at));
        AMediaDataSource_setGetSize(data_source, Some(get_size));
        AMediaDataSource_setClose(data_source, Some(close));

        let media_extractor = AMediaExtractor_new();
        if media_extractor.is_null() {
            error!("`AMediaExtractor_new` failed");
            return;
        }

        AMediaExtractor_setDataSourceCustom(media_extractor, data_source);

        let media_format = AMediaExtractor_getTrackFormat(media_extractor, 0);
        if media_format.is_null() {
            error!("`AMediaExtractor_getTrackFormat` failed");
        } else {
            info!("`AMediaExtractor_getTrackFormat` returned something");
            use ndk_sys::{AMediaFormat_getInt32};
            use std::ffi::CStr;

            let mut height = 0;
            let mut width = 0;
            AMediaFormat_getInt32(media_format, AMEDIAFORMAT_KEY_HEIGHT, &mut height);
            AMediaFormat_getInt32(media_format, AMEDIAFORMAT_KEY_WIDTH, &mut width);

            info!("{} {}", height, width);

            // let mut mime_type: *const i8 = std::ptr::null_mut();
            // AMediaFormat_getString(media_format, AMEDIAFORMAT_KEY_MIME, &mut mime_type);
            // let cstr = CStr::from_ptr(mime_type);
            // info!("mime: {}", cstr.to_string_lossy());
            // AMediaFormat_delete(media_format);
        }
        AMediaExtractor_delete(media_extractor);
        AMediaDataSource_close(data_source);
    }
}

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
    ) -> anyhow::Result<Self> {
        unsafe {
            if let Some(media_format) = NonNull::new(AMediaFormat_new()) {
                AMediaFormat_setString(
                    media_format.as_ptr(),
                    AMEDIAFORMAT_KEY_MIME,
                    video_type.as_cstr_ptr(),
                );
                AMediaFormat_setInt32(media_format.as_ptr(), AMEDIAFORMAT_KEY_HEIGHT, height);
                AMediaFormat_setInt32(media_format.as_ptr(), AMEDIAFORMAT_KEY_WIDTH, width);
                AMediaFormat_setInt32(
                    media_format.as_ptr(),
                    AMEDIAFORMAT_KEY_FRAME_RATE,
                    frame_rate,
                );
                Ok(MediaFormat(media_format))
            } else {
                anyhow::bail!("AMediaFormat_new returned a null");
            }
        }
    }
}

pub(crate) enum VideoType {
    H264,
    HEVC,
}

impl VideoType {
    pub(crate) fn as_cstr_ptr(&self) -> *const std::os::raw::c_char {
        self.mime_cstr().as_ptr().cast()
    }

    fn mime_cstr(&self) -> &'static str {
        match self {
            VideoType::H264 => "video/avc\0",
            VideoType::HEVC => "video/hevc\0",
        }
    }
}
