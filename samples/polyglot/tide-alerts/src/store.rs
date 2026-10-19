//! the only module that knows where alert facts live.
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone)]
pub struct Reading {
    pub berth: String,
    pub water_level_cm: i32,
}

pub fn pool() -> Mutex<HashMap<String, Reading>> {
    Mutex::new(HashMap::new())
}

pub fn save(pool: &Mutex<HashMap<String, Reading>>, reading: &Reading) -> Reading {
    pool.lock().unwrap().insert(reading.berth.clone(), reading.clone());
    reading.clone()
}

pub fn find(pool: &Mutex<HashMap<String, Reading>>, berth: &str) -> Option<Reading> {
    pool.lock().unwrap().get(berth).cloned()
}
