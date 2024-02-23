use skyline::libc::memcpy;

use crate::navigation::{self, CurrentNavigation};
use crate::playaid;

static KEYBOARD_OFFSET: usize = 0x39c5380;

#[skyline::hook(offset = KEYBOARD_OFFSET)] 
pub unsafe fn show_keyboard(string: *mut *mut u16, _show_keyboard_arg: *const u64) -> u32 {
    let return_code = 0;
    println!("Adding into keyboard the id: {}", playaid::TEST_ID[playaid::ID_INDEX]);
    let new_string_vec: Vec<u16> = playaid::TEST_ID[playaid::ID_INDEX].encode_utf16().chain(core::iter::once(0)).collect();
    let raw_vec_info = new_string_vec.into_raw_parts();
    memcpy(*string as _, raw_vec_info.0 as _, raw_vec_info.2 * 2);
    increment_id_index();
    navigation::NAV = CurrentNavigation::ScSearchResults;
    return_code
}

pub fn init() {
    skyline::install_hooks!(
        show_keyboard
    );
}

unsafe fn _utf16_to_string(char_ptr: *const u16) -> String {
    let mut len = 0;
    while *(char_ptr.add(len)) != 0 {
        len += 1;
    }
    let utf16_slice: &[u16] = std::slice::from_raw_parts(char_ptr, len);
    let rust_string = String::from_utf16_lossy(utf16_slice);
    rust_string
}

pub unsafe fn increment_id_index() {
    playaid::ID_INDEX += 1;
    if playaid::ID_INDEX >= playaid::TEST_ID.len() {
        playaid::ID_INDEX = 0;
        playaid::final_replay();
    }
}