#![feature(repr_simd)]
#![feature(simd_ffi)]

use skyline;
use acmd::acmd;
// use smash::lib::lua_const::*;
use smash::phx::{ Vector3f };
// use smash::app::lua_bind::*;
use smash::lib::{ lua_const, L2CValue };
// use smash::app::{ utility, sv_system, smashball, Item, lua_bind::*, self };
use smash::app::{ utility, sv_system, smashball, Item };
use smash::cpp::l2c_value::LuaConst;
use smash::hash40;
use smash::phx::{ Hash40 };
use smash::lua2cpp::{ L2CFighterBase, L2CFighterBase_global_reset };
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
static mut ITEM_MANAGER_ADDR: usize = 0;

// 0 - we haven't started logging.
// 1 - we are actively logging.
// 2 - we have finished logging.
static LOGGING_STATE: AtomicU32 = AtomicU32::new(0);

// Don't remove Mii hats, Pikmin, Luma, or crafting table
const ARTICLE_ALLOWLIST: [(LuaConst, LuaConst); 8] = [
    (FIGHTER_KIND_MIIFIGHTER, FIGHTER_MIIFIGHTER_GENERATE_ARTICLE_HAT),
    (FIGHTER_KIND_MIISWORDSMAN, FIGHTER_MIISWORDSMAN_GENERATE_ARTICLE_HAT),
    (FIGHTER_KIND_MIIGUNNER, FIGHTER_MIIGUNNER_GENERATE_ARTICLE_HAT),
    (FIGHTER_KIND_ROSETTA, FIGHTER_ROSETTA_GENERATE_ARTICLE_TICO),
    (FIGHTER_KIND_PICKEL, FIGHTER_PICKEL_GENERATE_ARTICLE_TABLE),
    (FIGHTER_KIND_ELIGHT, FIGHTER_ELIGHT_GENERATE_ARTICLE_ESWORD),
    (FIGHTER_KIND_EFLAME, FIGHTER_EFLAME_GENERATE_ARTICLE_ESWORD),
    (FIGHTER_KIND_PIKMIN, FIGHTER_PIKMIN_GENERATE_ARTICLE_PIKMIN),
];

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
    let fighter_manager = unsafe { *(FIGHTER_MANAGER_ADDR as *mut *mut app::FighterManager) };
    let is_ready_go = unsafe { FighterManager::is_ready_go(fighter_manager) };
    let is_result_mode = unsafe { FighterManager::is_result_mode(fighter_manager) };

    if !is_ready_go && !is_result_mode && LOGGING_STATE.load(Ordering::SeqCst) != 1 {
        // We are in the starting state, it's time to create a log.
        println!("Starting");
        LOGGING_STATE.store(1, Ordering::SeqCst);
    }

    if is_result_mode && LOGGING_STATE.load(Ordering::SeqCst) == 1 {
        println!("Flushing to log!");
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
        *file_path = format!("sd:/fight-{}-vs-{}-{}.txt", fighter1, fighter2, event_time);
        File::create(&*file_path);

        let file = OpenOptions::new().write(true).append(true).open(&*file_path);

        let mut file = match file {
            Err(e) => panic!("Couldn't open file: {}", e),
            Ok(file) => file,
        };

        if let Err(e) = write!(file, "{}", buffer.as_str()) {
            panic!("Couldn't write to file: {}", e);
        }

        println!("Wrote to {}", file_path.to_string());
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
    CancelModule::is_enable_cancel(module_accessor) ||
        actionable_statuses!()
            .iter()
            .any(|actionable_transition| {
                WorkModule::is_enable_transition_term(module_accessor, **actionable_transition)
            })
}

pub unsafe fn record_articles(
    fighter_kind: i32,
    module_accessor: &mut app::BattleObjectModuleAccessor
) {
    SoundModule::stop_all_sound(module_accessor);
    // All articles have ID <= 0x25
    (0..=0x25)
        .filter(|article_idx| {
            !ARTICLE_ALLOWLIST.iter().any(|article_allowed| {
                article_allowed.0 == fighter_kind && article_allowed.1 == *article_idx
            })
        })
        .for_each(|article_idx| {
            if ArticleModule::is_exist(module_accessor, article_idx) {
                let article: *mut app::Article = ArticleModule::get_article(
                    module_accessor,
                    article_idx
                );
                // let article_object_id = Article::get_battle_object_id(article);
                // ArticleModule::remove_exist_object_id(module_accessor, article_object_id as u32);
            }
        });
    let item_mgr = *(ITEM_MANAGER_ADDR as *mut *mut app::ItemManager);
    (0..ItemManager::get_num_of_active_item_all(item_mgr)).for_each(|item_idx| {
        let item = ItemManager::get_active_item(item_mgr, item_idx);
        if item != 0 {
            let item = item as *mut Item;
            // let item_battle_object_id = app::Item::get_battle_object_id(item) as u32;
            // ItemManager::remove_item_from_id(item_mgr, item_battle_object_id);
        }
    });
    MotionAnimcmdModule::set_sleep(module_accessor, true);
    SoundModule::pause_se_all(module_accessor, true);
    ControlModule::stop_rumble(module_accessor, true);
    SoundModule::stop_all_sound(module_accessor);
    // Return camera to normal when loading save state
    SlowModule::clear_whole(module_accessor);
    CameraModule::zoom_out(module_accessor, 0);
    // Remove blue effect (but does not remove darkened screen)
    EffectModule::kill_kind(module_accessor, Hash40::new("sys_bg_criticalhit"), false, false);
    // Removes the darkened screen from special zooms
    // If there's a crit that doesn't get removed, it's likely bg_criticalhit2.
    EffectModule::remove_screen(module_accessor, Hash40::new("bg_criticalhit"), 0);
    // Remove all quakes to prevent screen shake lingering through load.
    for quake_kind in *CAMERA_QUAKE_KIND_NONE..=*CAMERA_QUAKE_KIND_MAX {
        CameraModule::stop_quake(module_accessor, quake_kind);
    }
}

pub fn once_per_frame_per_fighter(fighter: &mut smash::common::root::lua2cpp::L2CFighterCommon) {
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
        let game_started = FighterManager::is_ready_go(fighter_manager);

        if !game_started {
            // println!("Game not ready yet");
            return;
        }

        let num_frames_left = get_remaining_time_as_frame();

        let fighter_id = WorkModule::get_int(
            module_accessor,
            *lua_const::FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID
        ) as i32;

        let fighter_information = FighterManager::get_fighter_information(
            fighter_manager,
            app::FighterEntryID(fighter_id)
        ) as *mut app::FighterInformation;
        let stock_count = FighterInformation::stock_count(fighter_information) as u8;
        let fighter_status_kind = StatusModule::status_kind(module_accessor);
        let fighter_name = utility::get_kind(module_accessor);
        let fighter_motion_kind = MotionModule::motion_kind(module_accessor);
        let fighter_damage = DamageModule::damage(module_accessor, 0);
        let fighter_shield_size = WorkModule::get_float(
            module_accessor,
            *lua_const::FIGHTER_INSTANCE_WORK_ID_FLOAT_GUARD_SHIELD
        );
        let attack_connected = AttackModule::is_infliction_status(
            module_accessor,
            *lua_const::COLLISION_KIND_MASK_HIT
        );
        let hitstun_left = WorkModule::get_float(
            module_accessor,
            *lua_const::FIGHTER_INSTANCE_WORK_ID_FLOAT_DAMAGE_REACTION_FRAME
        );
        let can_act = can_act(module_accessor);
        let pos_x = PostureModule::pos_x(module_accessor);
        let pos_y = PostureModule::pos_y(module_accessor);
        let facing = PostureModule::lr(module_accessor);
        let cam_pos = get_camera_pos();
        let internal_cam_pos = get_internal_camera_pos();
        let cam_target = get_camera_target();
        let cam_fov = get_camera_fov();
        let stage_id = get_stage_id();
        let animation_frame_num = MotionModule::frame(module_accessor);

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

        // Write buffer to file every 50 frames
        let FLUSH_ON_INTERVAL = false;
        if FLUSH_ON_INTERVAL && *fighter_log_count % 100 == 0 {
            let mut file_path = FILE_PATH.lock().unwrap();

            if file_path.is_empty() {
                println!("FILE_PATH is empty, giving it a new path");
                let fighter1 = FIGHTER_1.lock().unwrap();

                let fighter2 = FIGHTER_2.lock().unwrap();

                *file_path = format!(
                    "sd:/fight-{}-vs-{}-{}-{}-{}-take3.txt",
                    fighter1,
                    fighter2,
                    fighter_motion_kind,
                    pos_x,
                    pos_y
                );
                File::create(&*file_path);
            }

            let file = OpenOptions::new().write(true).append(true).open(&*file_path);

            let mut file = match file {
                Err(e) => panic!("Couldn't open file: {}", e),
                Ok(file) => file,
            };

            if let Err(e) = write!(file, "{}", buffer.as_str()) {
                panic!("Couldn't write to file: {}", e);
            }

            // Clear the buffer after writing
            buffer.clear();
        }
    }
}

// Explicitly cast the function item to a function pointer
const once_per_frame_per_fighter_ptr: fn(&mut smash::common::root::lua2cpp::L2CFighterCommon) = once_per_frame_per_fighter;

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
    println!("v13 - Playaid Logger");

    unsafe {
        skyline::nn::ro::LookupSymbol(
            &mut FIGHTER_MANAGER_ADDR,
            "_ZN3lib9SingletonIN3app14FighterManagerEE9instance_E\u{0}".as_bytes().as_ptr()
        );
    }

    unsafe {
        skyline::nn::ro::LookupSymbol(
            &mut ITEM_MANAGER_ADDR,
            "_ZN3lib9SingletonIN3app11ItemManagerEE9instance_E\0".as_bytes().as_ptr()
        );
    }

    skyline::nro::add_hook(nro_main).unwrap();

    acmd::add_custom_hooks!(once_per_frame_per_fighter_ptr);
}
