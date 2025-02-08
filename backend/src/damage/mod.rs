pub mod character;
pub mod entity;
pub mod general;
pub mod weapon;
use actix_web::web;


pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(general::get_overall_damage_info);
    cfg.service(weapon::get_damage_weapon);
    cfg.service(character::get_damage_character);
    cfg.service(entity::get_damage_entity);
}
