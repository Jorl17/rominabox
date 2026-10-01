//! The sandbox (AppContainer) of a Windows game, which we register, find and
//! remove in the tests. We name it from the game's identity, as in the launcher.
#![allow(dead_code)]

use std::ffi::c_void;

#[link(name = "userenv")]
extern "system" {
    fn CreateAppContainerProfile(
        name: *const u16,
        display: *const u16,
        description: *const u16,
        capabilities: *const c_void,
        count: u32,
        sid: *mut *mut c_void,
    ) -> i32;
    fn DeleteAppContainerProfile(name: *const u16) -> i32;
    fn DeriveAppContainerSidFromAppContainerName(name: *const u16, sid: *mut *mut c_void) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertSidToStringSidW(sid: *mut c_void, text: *mut *mut u16) -> i32;
    fn FreeSid(sid: *mut c_void) -> *mut c_void;
    fn RegOpenKeyExW(key: isize, subkey: *const u16, options: u32, access: u32, opened: *mut isize) -> i32;
    fn RegCloseKey(key: isize) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
}

pub const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
const KEY_READ: u32 = 0x2_0019;

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A name used by both the launcher and the player, from rominabox_launch.h.
pub fn declared(name: &str) -> String {
    let header =
        std::fs::read_to_string(rominabox_desktop::repo::at("vendor/retroarch/rominabox_launch.h")).unwrap();
    let start = format!("#define {name} \"");
    header
        .lines()
        .find_map(|line| line.strip_prefix(&start)?.strip_suffix('"').map(str::to_owned))
        .unwrap_or_else(|| panic!("rominabox_launch.h declares no {name}"))
}

/// The name of the game's sandbox, as we form it in the launcher.
pub fn sandbox_name(identity: &str) -> String {
    format!("{}{identity}", declared("RIB_GAME_APP_ID_PREFIX"))
}

/// Register the game's sandbox for this user, as on the game's first launch.
pub fn register(identity: &str) {
    let name = wide(&sandbox_name(identity));
    let mut sid = std::ptr::null_mut();
    let made = unsafe { CreateAppContainerProfile(name.as_ptr(), name.as_ptr(), name.as_ptr(), std::ptr::null(), 0, &mut sid) };
    assert!(made >= 0, "Windows did not register {}: {made:#x}", sandbox_name(identity));
    unsafe {
        FreeSid(sid);
    }
}

/// Remove the game's sandbox, its registration and its folder, if it exists.
pub fn unregister(identity: &str) {
    let name = wide(&sandbox_name(identity));
    unsafe {
        DeleteAppContainerProfile(name.as_ptr());
    }
}

/// Whether the game's sandbox is registered for this user. In the registry,
/// the name of each registered sandbox is under its SID.
pub fn registered(identity: &str) -> bool {
    let name = wide(&sandbox_name(identity));
    let mut sid = std::ptr::null_mut();
    let mut text = std::ptr::null_mut();
    unsafe {
        assert!(DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) >= 0);
        assert!(ConvertSidToStringSidW(sid, &mut text) != 0);
        FreeSid(sid);
    }
    let length = (0..).take_while(|&at| unsafe { *text.add(at) } != 0).count();
    let sid_text = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).unwrap();
    unsafe {
        LocalFree(text.cast());
    }
    let key = wide(&format!(
        r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppContainer\Mappings\{sid_text}"
    ));
    let mut opened = 0;
    let found = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_READ, &mut opened) } == 0;
    if found {
        unsafe {
            RegCloseKey(opened);
        }
    }
    found
}
