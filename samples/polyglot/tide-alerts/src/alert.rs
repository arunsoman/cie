//! handlers for the alert API.
use crate::store::{self, Reading};

/// a reading past 220 cm means the berth floods.

pub fn level_for(berth: &str) -> i32 {
    if berth.starts_with("premium") {
        240
    } else {
        190
    }
}

pub async fn current_alert(axum::extract::Path(berth): axum::extract::Path<String>) -> String {
    let water = level_for(&berth);
    if water > 220 {
        format!("flood risk at {} — level {} cm", berth, water)
    } else {
        format!("no alert at {} — level {} cm", berth, water)
    }
}

pub async fn check(axum::extract::Path(berth): axum::extract::Path<String>) -> String {
    let water = level_for(&berth);
    let reading = Reading { berth: berth.clone(), water_level_cm: water };
    store::save(&store::pool(), &reading);
    if water <= 200 {
        return format!("{} below 200 — safe", berth);
    }
    format!("{} recorded — {} cm", berth, water)
}
