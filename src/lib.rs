#![feature(repr_simd)]
#![feature(simd_ffi)]

#![feature(pointer_byte_offsets)]
#![feature(new_uninit)]
#![feature(vec_into_raw_parts)]
use crate::navigation::CurrentNavigation;

mod navigation;
mod input;
mod keyboard;
mod playaid;

use skyline;
use acmd::acmd;
use smash::lib::lua_const::*;
use smash::app;
use smash::phx::{ Vector3f };
use smash::app::lua_bind;
use smash::lib::{ lua_const, L2CValue };
use smash::app::{ utility, sv_system, smashball };
use smash::hash40;
use smash::lua2cpp::{ L2CFighterCommon, L2CFighterBase, L2CFighterBase_global_reset };
use serde_json::json;
use std::sync::atomic::{ AtomicU32, Ordering };
use std::sync::Mutex;
use std::fs::File;
use std::fs::OpenOptions;
use lazy_static::lazy_static;
use std::io::Write;
use std::cell::RefCell;
use std::rc::Rc;
use skyline::nn::{ time };
use std::time::{ SystemTime, UNIX_EPOCH };

lazy_static! {
    static ref FILE_PATH: Mutex<String> = Mutex::new(String::new());
    static ref FIGHTER_LOG_COUNT: Mutex<usize> = Mutex::new(0);
    static ref BUFFER: Mutex<String> = Mutex::new(String::new());
    static ref FIGHTER_1: Mutex<String> = Mutex::new(String::new());
    static ref FIGHTER_2: Mutex<String> = Mutex::new(String::new());
}

#[repr(simd)]
pub struct SimdVector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

extern "C" {
    #[link_name = "\u{1}_ZN3app14sv_information27get_remaining_time_as_frameEv"]
    pub fn get_remaining_time_as_frame() -> u32;

    #[link_name = "\u{1}_ZN3app14sv_information8stage_idEv"]
    pub fn get_stage_id() -> i32;

    #[link_name = "\u{1}_ZN3app17sv_camera_manager7get_posEv"]
    pub fn get_camera_pos() -> SimdVector3;

    #[link_name = "\u{1}_ZN3app17sv_camera_manager16get_internal_posEv"]
    pub fn get_internal_camera_pos() -> SimdVector3;

    #[link_name = "\u{1}_ZN3app17sv_camera_manager10get_targetEv"]
    pub fn get_camera_target() -> SimdVector3;

    #[link_name = "\u{1}_ZN3app17sv_camera_manager7get_fovEv"]
    pub fn get_camera_fov() -> f32;
}

static mut FIGHTER_MANAGER_ADDR: usize = 0;

// 0 - we haven't started logging.
// 1 - we are actively logging.
// 2 - we have finished logging.
static LOGGING_STATE: AtomicU32 = AtomicU32::new(0);

// This gets called whenever a match starts or ends. Still gets called once per fighter which is odd.
// A typical fight will have the following logs.
//   HIT on_match_start_or_end
//   HIT on_match_start_or_end
//   HIT on_match_start_or_end
//   HIT on_match_start_or_end
//   HIT on_match_start_or_end
//   In is_ready_go
//   HIT on_match_start_or_end
//   In is_ready_go
//   ...
//   HIT on_match_start_or_end
//   In is_ready_go
//   ... once the match ends ..
//   HIT on_match_start_or_end
//   In is_result_mode
//   HIT on_match_start_or_end
//   In is_result_mode
//   HIT on_match_start_or_end
//   In is_result_mode
#[skyline::hook(replace = L2CFighterBase_global_reset)]
pub fn on_match_start_or_end(fighter: &mut L2CFighterBase) -> L2CValue {
    println!("[ult-logger] Hit on_match_start_or_end with logging_state = {}", LOGGING_STATE.load(Ordering::SeqCst));
    let fighter_manager = unsafe { *(FIGHTER_MANAGER_ADDR as *mut *mut app::FighterManager) };
    let is_ready_go = unsafe { lua_bind::FighterManager::is_ready_go(fighter_manager) };
    let is_result_mode = unsafe { lua_bind::FighterManager::is_result_mode(fighter_manager) };

    if !is_ready_go && !is_result_mode && LOGGING_STATE.load(Ordering::SeqCst) != 1 {
        // We are in the starting state, it's time to create a log.
        println!("[ult-logger] Starting");
        LOGGING_STATE.store(1, Ordering::SeqCst);
    }

    if is_result_mode && LOGGING_STATE.load(Ordering::SeqCst) == 1 {
        println!("[ult-logger] Flushing to log!");
        LOGGING_STATE.store(2, Ordering::SeqCst);

        let mut buffer = BUFFER.lock().unwrap();

        let mut file_path = FILE_PATH.lock().unwrap();

        let fighter1 = FIGHTER_1.lock().unwrap();

        let fighter2 = FIGHTER_2.lock().unwrap();

        unsafe {
            time::Initialize();
        }
        let event_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        // *file_path = format!("sd:/fight-{}-vs-{}-{}.txt", fighter1, fighter2, event_time);

        let mut replay_id = "XXXXXXXX";
        unsafe {
            // This is index - 1 because we increment right after we input info into the keyboard.
            // This is sloppy code.
            replay_id = playaid::TEST_ID[playaid::ID_INDEX - 1];
        }

        *file_path = format!("sd:/{}-{}.txt", replay_id, event_time);
        File::create(&*file_path);

        let file = OpenOptions::new().write(true).append(true).open(&*file_path);

        let mut file = match file {
            Err(e) => panic!("Couldn't open file: {}", e),
            Ok(file) => file,
        };

        if let Err(e) = write!(file, "{}", buffer.as_str()) {
            panic!("Couldn't write to file: {}", e);
        }

        println!("[ult-logger] Wrote to {}", file_path.to_string());
        // Clear the buffer after writing
        buffer.clear();
    }

    original!()(fighter)
}

macro_rules! actionable_statuses {
    () => {
        vec![
            FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ESCAPE_AIR,
            FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ATTACK_AIR,
            FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_GUARD_ON,
            FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ESCAPE,
        ]
    };
}

unsafe fn can_act(module_accessor: *mut app::BattleObjectModuleAccessor) -> bool {
    smash::app::lua_bind::CancelModule::is_enable_cancel(module_accessor) ||
        actionable_statuses!()
            .iter()
            .any(|actionable_transition| {
                smash::app::lua_bind::WorkModule::is_enable_transition_term(
                    module_accessor,
                    **actionable_transition
                )
            })
}

pub fn once_per_frame_per_fighter(fighter: &mut L2CFighterCommon) {
    let mut fighter_log_count = FIGHTER_LOG_COUNT.lock().unwrap();
    *fighter_log_count += 1;

    let mut buffer = BUFFER.lock().unwrap();

    unsafe {
        let module_accessor = smash::app::sv_system::battle_object_module_accessor(
            fighter.lua_state_agent
        );

        let fighter_manager = *(FIGHTER_MANAGER_ADDR as *mut *mut app::FighterManager);

        // If True, the game has started and the characters can move around.  Otherwise, it's still loading with the
        // countdown.
        let game_started = lua_bind::FighterManager::is_ready_go(fighter_manager);

        if !game_started {
            // println!("[ult-logger] Game not ready yet");
            return;
        }

        let num_frames_left = get_remaining_time_as_frame();

        let fighter_id = lua_bind::WorkModule::get_int(
            module_accessor,
            *lua_const::FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID
        ) as i32;

        let fighter_information = lua_bind::FighterManager::get_fighter_information(
            fighter_manager,
            app::FighterEntryID(fighter_id)
        ) as *mut app::FighterInformation;
        let stock_count = lua_bind::FighterInformation::stock_count(fighter_information) as u8;
        let fighter_status_kind = lua_bind::StatusModule::status_kind(module_accessor);
        let fighter_name = utility::get_kind(module_accessor);
        let fighter_motion_kind = lua_bind::MotionModule::motion_kind(module_accessor);
        let fighter_damage = lua_bind::DamageModule::damage(module_accessor, 0);
        let fighter_shield_size = lua_bind::WorkModule::get_float(
            module_accessor,
            *lua_const::FIGHTER_INSTANCE_WORK_ID_FLOAT_GUARD_SHIELD
        );
        let attack_connected = lua_bind::AttackModule::is_infliction_status(
            module_accessor,
            *lua_const::COLLISION_KIND_MASK_HIT
        );
        let hitstun_left = lua_bind::WorkModule::get_float(
            module_accessor,
            *lua_const::FIGHTER_INSTANCE_WORK_ID_FLOAT_DAMAGE_REACTION_FRAME
        );
        let can_act = can_act(module_accessor);
        let pos_x = lua_bind::PostureModule::pos_x(module_accessor);
        let pos_y = lua_bind::PostureModule::pos_y(module_accessor);
        let facing = lua_bind::PostureModule::lr(module_accessor);
        let cam_pos = get_camera_pos();
        let internal_cam_pos = get_internal_camera_pos();
        let cam_target = get_camera_target();
        let cam_fov = get_camera_fov();
        let stage_id = get_stage_id();
        let animation_frame_num = smash::app::lua_bind::MotionModule::frame(module_accessor);

        if fighter_id == 0 {
            let mut fighter1 = FIGHTER_1.lock().unwrap();
            *fighter1 = format!("{}", fighter_name);
        }

        if fighter_id == 1 {
            let mut fighter2 = FIGHTER_2.lock().unwrap();
            *fighter2 = format!("{}", fighter_name);
        }

        let json_log =
            json!({
            "num_frames_left": num_frames_left,
            "fighter_id": fighter_id,
            "fighter_name": fighter_name,
            "stock_count": stock_count,
            "status_kind": fighter_status_kind,
            "motion_kind": fighter_motion_kind,
            "damage": fighter_damage,
            "shield_size": fighter_shield_size,
            "facing": facing,
            "pos_x": pos_x,
            "pos_y": pos_y,
            "hitstun_left": hitstun_left,
            "attack_connected": attack_connected,
            "animation_frame_num": animation_frame_num,
            "can_act": can_act,
            "camera_position": {
                "x": cam_pos.x,
                "y": cam_pos.y,
                "z": cam_pos.z,
            },
            "camera_target_position": {
                "x": cam_target.x,
                "y": cam_target.y,
                "z": cam_target.z,
            },
            "camera_fov": cam_fov,
            "stage_id": stage_id,
        });

        let PUSH_TO_BUFFER = true;
        if PUSH_TO_BUFFER {
            buffer.push_str(&format!("{}\n", json_log));
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct FixedBaseString<const N: usize> {
    fnv: u32,
    string_len: u32,
    string: [u8; N],
}

#[repr(C)]
#[derive(Debug)]
pub struct SceneQueue {
    end: *const u64,
    start: *const u64,
    count: usize,
    active_scene: FixedBaseString<64>,
    previous_scene: FixedBaseString<64>,
}

#[skyline::hook(offset = 0x3724c10)]
fn change_scene_sequence(
    queue: &SceneQueue,
    fnv1: &mut FixedBaseString<64>,
    fnv2: &mut FixedBaseString<64>,
    parameters: *const u8
) {
    if
        &fnv1.string[0..24] == b"OnlineShareSequenceScene" &&
        &fnv2.string[0..17] == b"MenuSequenceScene"
    {
        println!("[ult-logger] Made it to Shared Content!");
        unsafe {
            navigation::NAV = CurrentNavigation::ScWaitingForLoad;
        }
    }
    call_original!(queue, fnv1, fnv2, parameters);
}

#[skyline::from_offset(0x39c4bb0)]
fn begin_auto_sleep_disabled();

#[skyline::hook(offset = 0x39c4bd0)]
fn end_auto_sleep_disabled() {
    // We don't want to auto-sleep ever, so don't let this end
}

#[skyline::hook(offset = 0x39c4bc0)]
fn kill_backlight() {
    // We don't want to kill backlight ever, so don't let this happen
}

fn hook_panic() {
    std::panic::set_hook(
        Box::new(|info| {
            let location = info.location().unwrap();

            let msg = match info.payload().downcast_ref::<&'static str>() {
                Some(s) => *s,
                None => {
                    match info.payload().downcast_ref::<String>() {
                        Some(s) => &s[..],
                        None => "Box<Any>",
                    }
                }
            };

            let err_msg = format!("thread has panicked at '{}', {}", msg, location);
            skyline::error::show_error(
                69,
                "Skyline plugin has panicked! Please open the details and send a screenshot to the developer, then close the game.\n\0",
                err_msg.as_str()
            );
        })
    );
}

// Use this for general per-frame weapon-level hooks
// Reference: https://gist.github.com/jugeeya/27b902865408c916b1fcacc486157f79
pub fn once_per_weapon_frame(fighter_base: &mut L2CFighterBase) {
    unsafe {
        let module_accessor = smash::app::sv_system::battle_object_module_accessor(
            fighter_base.lua_state_agent
        );
        println!("[ult-logger] Frame : {}", smash::app::lua_bind::MotionModule::frame(module_accessor));
    }
}

fn nro_main(nro: &skyline::nro::NroInfo<'_>) {
    match nro.name {
        "common" => {
            skyline::install_hooks!(on_match_start_or_end);
        }
        _ => (),
    }
}

#[skyline::main(name = "ult_logger")]
pub fn main() {
    println!("[ult-logger] !!! v16 !!!");

    unsafe {
        skyline::nn::ro::LookupSymbol(
            &mut FIGHTER_MANAGER_ADDR,
            "_ZN3lib9SingletonIN3app14FighterManagerEE9instance_E\u{0}".as_bytes().as_ptr()
        );
    }

    skyline::nro::add_hook(nro_main).unwrap();

    acmd::add_custom_hooks!(once_per_frame_per_fighter);

    // Add panic hook
    hook_panic();

    // Initialize hooks for navigation and keyboard
    navigation::init();
    keyboard::init();

    // Initialize hooks for scene usage
    skyline::install_hooks!(change_scene_sequence, kill_backlight, end_auto_sleep_disabled);

    // Initialize hooks for input (from result_screen_skip)
    std::thread::sleep(std::time::Duration::from_secs(20)); //makes it not crash on startup with arcrop bc ???
    println!("[ult-logger] [Auto-Replay] Installing input hook...");
    unsafe {
        if (input::add_nn_hid_hook as *const ()).is_null() {
            panic!(
                "The NN-HID hook plugin could not be found and is required to add NRO hooks. Make sure libnn_hid_hook.nro is installed."
            );
        }
        input::add_nn_hid_hook(input::handle_get_npad_state_start);

        println!("[ult-logger] Disabling Auto Sleep");
        begin_auto_sleep_disabled()
    }
}
