use ndk_sys::{
    AAssetManager, AAssetManager_open, AAsset_close, AAsset_getRemainingLength64, AAsset_read,
    AASSET_MODE_STREAMING,
};
use std::ffi::CString;

fn read_asset(asset_manager: *mut AAssetManager, filename: &str) -> anyhow::Result<Vec<u8>> {
    let filename = CString::new(filename).unwrap();
    let asset =
        unsafe { AAssetManager_open(asset_manager, filename.as_ptr(), AASSET_MODE_STREAMING as _) };
    if asset.is_null() {
        anyhow::bail!("{} not found", filename.into_string().unwrap());
    } else {
        let buf_size = unsafe { AAsset_getRemainingLength64(asset) };
        let mut buf = vec![0; buf_size as usize];
        unsafe {
            let bytes = AAsset_read(asset, buf.as_mut_ptr().cast(), buf_size as u64);
            AAsset_close(asset);
            if bytes as i64 != buf_size {
                anyhow::bail!("Failed reading {}", filename.into_string().unwrap());
            }
        }
        Ok(buf)
    }
}

pub fn get_h264_packets(asset_manager: *mut AAssetManager) -> anyhow::Result<Vec<Vec<u8>>> {
    let mut packets = Vec::new();
    for i in 0..120 {
        let buf = read_asset(asset_manager, &format!("{}.h264", i))?;
        packets.push(buf);
    }
    Ok(packets)
}

pub fn get_csd(asset_manager: *mut AAssetManager) -> anyhow::Result<Vec<u8>> {
    read_asset(asset_manager, "csd.bin")
}
