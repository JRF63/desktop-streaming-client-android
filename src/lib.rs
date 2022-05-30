mod decoder;
mod log;
mod media_format;

mod debug;

use ndk_sys::ANativeActivity;

// adb logcat -s client-android
// adb install target\debug\apk\client-android.apk
// C:\Users\Rafael\AppData\Local\Android\Sdk\emulator\emulator -avd Pixel_3_API_31

#[no_mangle]
unsafe extern "C" fn ANativeActivity_onCreate(
    activity: *mut ANativeActivity,
    _saved_state: *mut u8,
    _saved_state_size: usize,
) {
    // need to set the ANativeActivity callbacks, particularly for ANativeWindow
    info!("starting1");

    // if let Ok(csd) = debug::get_csd((&*activity).assetManager) {
    //     info!("{}", csd.len());
    // } else {
    //     error!("`get_csd` failed");
    // }

    match debug::get_csd((&*activity).assetManager) {
        Ok(csd) => {
            media_format::media_format_from_sample(&csd);
        }
        Err(e) => error!("{}", e),
    }

    info!("exiting");
}
