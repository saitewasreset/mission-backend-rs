use tabled::{Table, Tabled};
use common::mission::MissionKPIInfoFull;

#[derive(Tabled)]
pub struct MissionKPIPlayerGeneralTableEntry {
    pub player_name: String,
    #[tabled(rename = "character")]
    pub kpi_character_type: String,
    #[tabled(format = "{:.0}")]
    #[tabled(rename = "w_kill")]
    pub weighted_kill: f64,
    #[tabled(format = "{:.2}")]
    #[tabled(rename = "w_damage")]
    pub weighted_damage: f64,
    #[tabled(format = "{:.2}")]
    #[tabled(rename = "priority")]
    pub priority_damage: f64,
    #[tabled(format = "{:.0}")]
    #[tabled(rename = "revive")]
    pub revive_num: f64,
    #[tabled(format = "{:.0}")]
    #[tabled(rename = "death")]
    pub death_num: f64,
    #[tabled(format = "{:.2}")]
    #[tabled(rename = "ff")]
    pub friendly_fire: f64,
    #[tabled(format = "{:.2}")]
    pub nitra: f64,
    #[tabled(format = "{:.0}")]
    #[tabled(rename = "supply")]
    pub supply_count: f64,
    #[tabled(format = "{:.0}")]
    #[tabled(rename = "resource")]
    pub weighted_resource: f64,
}

#[derive(Tabled)]
pub struct MissionKPIPlayerComponentTableEntry {
    pub name: String,
    #[tabled(format = "{:.0}")]
    #[tabled(rename = "source")]
    pub source_value: f64,
    #[tabled(format = "{:.0}")]
    #[tabled(rename = "weighted")]
    pub weighted_value: f64,
    #[tabled(format = "{:.0}")]
    #[tabled(rename = "total_weighted")]
    pub mission_total_weighted_value: f64,
    #[tabled(format = "{:.4}")]
    #[tabled(rename = "raw")]
    pub raw_index: f64,
    #[tabled(format = "{:.4}")]
    #[tabled(rename = "corrected")]
    pub corrected_index: f64,
    #[tabled(format = "{:.4}")]
    #[tabled(rename = "transformed")]
    pub transformed_index: f64,
    #[tabled(format = "{:.4}")]
    #[tabled(rename = "assigned")]
    pub assigned_index: f64,
    #[tabled(format = "{:.4}")]
    pub weight: f64,
}

pub fn format_mission_kpi_info(mission_kpi_info: MissionKPIInfoFull) -> (MissionKPIPlayerGeneralTableEntry, Vec<MissionKPIPlayerComponentTableEntry>) {
    let general_info = MissionKPIPlayerGeneralTableEntry {
        player_name: mission_kpi_info.player_name,
        kpi_character_type: mission_kpi_info.kpi_character_type,
        weighted_kill: mission_kpi_info.weighted_kill,
        weighted_damage: mission_kpi_info.weighted_damage,
        priority_damage: mission_kpi_info.priority_damage,
        revive_num: mission_kpi_info.revive_num,
        death_num: mission_kpi_info.death_num,
        friendly_fire: mission_kpi_info.friendly_fire,
        nitra: mission_kpi_info.nitra,
        supply_count: mission_kpi_info.supply_count,
        weighted_resource: mission_kpi_info.weighted_resource,
    };

    let component_info = mission_kpi_info.component.into_iter().map(|component| {
        MissionKPIPlayerComponentTableEntry {
            name: component.name,
            source_value: component.source_value,
            weighted_value: component.weighted_value,
            mission_total_weighted_value: component.mission_total_weighted_value,
            raw_index: component.raw_index,
            corrected_index: component.corrected_index,
            transformed_index: component.transformed_index,
            assigned_index: component.assigned_index,
            weight: component.weight,
        }
    }).collect();

    (general_info, component_info)
}

pub fn print_player_mission_kpi_info(mission_kpi_info: MissionKPIInfoFull) {
    let mission_kpi = mission_kpi_info.mission_kpi;
    let (general_info, component_info) = format_mission_kpi_info(mission_kpi_info);

    println!("player name: {}", general_info.player_name);
    println!("mission kpi: {:.4}", mission_kpi);
    println!("{}", Table::new(vec![general_info]));
    println!("{}", Table::new(&component_info));
}