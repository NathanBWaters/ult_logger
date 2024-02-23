use std::process;

// Statics for Replay IDs, populate these however from your sheets
pub static mut TEST_ID: &'static [&'static str] = &[

];

pub static mut ID_INDEX: usize = 0;

// Functions that may be useful to implement

// Called when the last ID (ID_INDEX - 1) was an invalid replay
pub fn handle_bad_id() {
    unsafe {
        println!("Hit a bad replay id: {}", TEST_ID[ID_INDEX - 1]);
    }
}

// Called when a replay is finished and we've returned to the ID selection
pub fn replay_done() {
    unsafe {
        println!("Completed the replay id: {}", TEST_ID[ID_INDEX - 1]);
    }
}

// Called when we're about to watch the last replay in the array
pub fn final_replay() {
    println!("We have played all of the replays");
    process::exit(0);
}