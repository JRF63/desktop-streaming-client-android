use std::fmt::format;

use ndk::{
    asset::{Asset, AssetManager},
    native_activity::NativeActivity,
};

// adb logcat -s client-android
// adb install target\debug\apk\client-android.apk

fn get_h264_packets(asset_manager: &AssetManager) -> Result<Vec<Asset>, Box<dyn std::error::Error>> {
    use std::ffi::CString;
    use std::io::Read;
    use std::ptr::NonNull;

    let packets = (0..120)
        .map(|i| {
            let filename = CString::new(format!("{}.h264", i)).unwrap();
            asset_manager.open(&filename).unwrap()
        })
        .collect::<Vec<Asset>>();

    Ok(packets)
}

#[cfg_attr(
    target_os = "android",
    ndk_glue::main(logger(level = "debug", tag = "client-android"))
)]
fn main() {
    // TODO: Probably don't use ndk_glue and ndk_context
    #[allow(deprecated)]
    let native_activity = ndk_glue::native_activity();

    let asset_manager = native_activity.asset_manager();
    let packets = get_h264_packets(&asset_manager).unwrap();
    log::info!("packets: {}", packets.len());
    // native_activity.finish();
}
