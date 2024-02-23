use skyline::nn::{self, hid::NpadHandheldState};
use rand::{self, Rng};

use crate::navigation::{self, CurrentNavigation};

pub fn handle_get_npad_state_start(
    state: *mut NpadHandheldState,
    _controller_id: *const u32,
) {
    unsafe {
        handle_menu_navigate(state);
    }
}

unsafe fn handle_menu_navigate(state: *mut NpadHandheldState) {
    // Keys to use for input
    let key_right: u64 = 0b0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0100_0000_0000_0000;
    let key_down: u64 = 0b0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_1000_0000_0000_0000;
    let key_up: u64 = 0b0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0010_0000_0000_0000;
    let key_x: u64 = 0b0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0100;
    let key_b: u64 = 0b0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0010;
    let key_a: u64 = 0b0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0001;
    let key_start: u64 = 0b0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0100_0000_0000;

    let mut key_to_press: u64 = 0;
    let mut key_to_alternate: u64 = 0;
    let mut key_to_hold: u64 = 0;

    if navigation::NAV == CurrentNavigation::ScVideo {
        println!("[input] Pressing B to get out of video playback");
        key_to_press = key_b;
    } else if navigation::NAV == CurrentNavigation::ScPlayback
    || navigation::NAV == CurrentNavigation::DoneHoverPlay {
        nn::oe::ReportUserIsActive(); // prevent switch from dimming
        key_to_press = key_b;
    } else if navigation::NAV == CurrentNavigation::MainOnMelee 
    || navigation::NAV == CurrentNavigation::MainInOnline
    {
        key_to_press = key_down;
    } else if navigation::NAV == CurrentNavigation::MainOnSpirits {
        key_to_press = key_right;
    } else if navigation::NAV == CurrentNavigation::ScWaitingForLoad {
        key_to_press = key_x;
    } else if navigation::NAV == CurrentNavigation::MainOnOnline ||
    navigation::NAV == CurrentNavigation::MainOnSharedContent ||
    navigation::NAV == CurrentNavigation::ScSearchSubmenuBottom ||
    navigation::NAV == CurrentNavigation::ScHoverReplay
    {
        key_to_press = key_a;
        nn::oe::ReportUserIsActive(); // prevent switch from dimming
    } else if navigation::NAV == CurrentNavigation::ScKeyboard {
        key_to_press = key_a; // press a and start to start typing keys and enter when possible
        key_to_alternate = key_start;
    } else if navigation::NAV == CurrentNavigation::ScWaitingForGame {
        key_to_hold = key_x;
    } else if navigation::NAV == CurrentNavigation::ScGO {
        if !navigation::should_wait() {
            key_to_hold = key_x | key_down; // press x+down to hide ui
            println!("input down to hide UI");
            navigation::NAV = CurrentNavigation::ScPlayback
        }
    }

    let mut rng = rand::thread_rng();
    // Need to space apart presses so it does not seem like we are holding the button.
    let n: u32 = rng.gen_range(0..3);
    if n == 1 {
        (*state).Buttons |= key_to_press;
    } else {
        (*state).Buttons |= key_to_alternate;
    }
    (*state).Buttons |= key_to_hold;

    if navigation::NAV == CurrentNavigation::ScSearchSubmenuTop {
        if !navigation::should_wait() {
            (*state).Buttons |= key_up;
            println!("Input up, delay reached!");
            navigation::NAV = CurrentNavigation::ScSearchSubmenuBottom;
        } 
    }
}

#[allow(improper_ctypes)]
extern "C" {
    pub fn add_nn_hid_hook(callback: fn(*mut NpadHandheldState,*const u32));
}