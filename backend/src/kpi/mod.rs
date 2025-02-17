pub mod bot_kpi_info;
pub mod info;
pub mod player;
pub mod version;
mod assigned_kpi;

use actix_web::web;
use std::{
    collections::HashMap,
};

pub fn apply_weight_table(
    source: &HashMap<String, f64>,
    weight_table: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    let mut result = HashMap::with_capacity(source.len());
    for (key, &value) in source {
        if let Some(&weight) = weight_table.get(key) {
            result.insert(key, value * weight);
        } else {
            result.insert(key, value);
        }
    }
    result.into_iter().map(|(k, v)| (k.clone(), v)).collect()
}

pub fn friendly_fire_index(ff_rate: f64) -> f64 {
    if ff_rate >= 0.91 {
        -1000.0
    } else {
        99.0 / (ff_rate - 1.0) + 100.0
    }
}

pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(info::get_gamma_info);
    cfg.service(info::get_transform_range_info);
    cfg.service(info::get_weight_table);

    cfg.service(version::get_kpi_version);

    cfg.service(player::get_player_kpi);

    cfg.service(bot_kpi_info::get_bot_kpi_info);

    cfg.service(assigned_kpi::api_get_assigned_kpi);
    cfg.service(assigned_kpi::api_set_assigned_kpi);
    cfg.service(assigned_kpi::api_delete_assigned_kpi);
}
