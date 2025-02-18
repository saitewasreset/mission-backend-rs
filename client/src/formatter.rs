use chrono::{Local, TimeZone, Utc};

pub fn format_timestamp_utc(timestamp: i64) -> String {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

pub fn format_timestamp_local(timestamp: i64) -> String {
    Local.timestamp_opt(timestamp, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

pub fn format_mission_time(mission_time: i16) -> String {
    let minutes = mission_time / 60;
    let seconds = mission_time % 60;

    format!("{}m{:02}s", minutes, seconds)
}

pub fn format_mission_result(result: i16) -> String {
    match result {
        0 => "Completed".to_string(),
        1 => "Failure".to_string(),
        2 => "Aborted".to_string(),
        _ => format!("Unknown({})", result),
    }
}