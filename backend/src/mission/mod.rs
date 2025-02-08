use actix_web::web;
pub mod load;
pub mod mission;
pub mod mission_list;

pub fn scoped_config(cfg: &mut web::ServiceConfig) {
    cfg.service(load::load_mission);
    cfg.service(mission_list::get_api_mission_list);
    cfg.service(mission_list::get_mission_list);

    cfg.service(mission::get_general_info);
    cfg.service(mission::get_mission_general);
    cfg.service(mission::get_mission_damage);
    cfg.service(mission::get_mission_weapon_damage);
    cfg.service(mission::get_mission_resource_info);
    cfg.service(mission::get_player_character);
    cfg.service(mission::get_mission_kpi);
    cfg.service(mission::get_mission_kpi_full);
}
