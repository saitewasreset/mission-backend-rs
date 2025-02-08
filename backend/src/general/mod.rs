pub mod character;
pub mod game_time;
pub mod general;
pub mod mission_type;
pub mod player;
use std::collections::HashMap;

use actix_web::web;
use serde::Serialize;


pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(general::get_general);
    cfg.service(mission_type::get_mission_type);
    cfg.service(player::get_player);
    cfg.service(character::get_character_general_info);
    cfg.service(character::get_character_choice_info);
    cfg.service(game_time::get_game_time);
}
