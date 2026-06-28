//! JNI entry for display cutout / safe-area insets and lifecycle pause from MainActivity.

use super::{set_app_paused, set_display_insets};

/// Called from Java: `MainActivity.nativeSetDisplayInsets(l, t, r, b)`
#[no_mangle]
pub extern "C" fn Java_com_hungerarena_app_MainActivity_nativeSetDisplayInsets(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) {
    set_display_insets(left, top, right, bottom);
}

/// Called from Java: `MainActivity.nativeSetPaused(bool)`
#[no_mangle]
pub extern "C" fn Java_com_hungerarena_app_MainActivity_nativeSetPaused(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    paused: u8,
) {
    set_app_paused_jni(paused != 0);
}

pub fn set_app_paused_jni(paused: bool) {
    set_app_paused(paused);
}

pub fn jni_on_load_insets() {}
