use tabled::{Table, Tabled};
use common::mission::{hazard_id_to_name, APIMission};
use crate::formatter::{format_mission_result, format_mission_time, format_timestamp_utc};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[derive(Tabled)]
pub struct MissionTableItem {
    pub id: i32,
    pub begin_time: String,
    pub mission_time: String,
    pub mission_type: String,
    pub hazard: String,
    pub result: String,
    pub reward_credit: String,
}

impl From<APIMission> for MissionTableItem {
    fn from(api_mission: APIMission) -> Self {
        let begin_time_str = format_timestamp_utc(api_mission.begin_timestamp);
        let mission_time = format_mission_time(api_mission.mission_time);
        let hazard_name = hazard_id_to_name(api_mission.hazard_id);

        MissionTableItem {
            id: api_mission.id,
            begin_time: begin_time_str,
            mission_time,
            mission_type: api_mission.mission_type,
            hazard: hazard_name,
            result: format_mission_result(api_mission.result),
            reward_credit: format!("{}", api_mission.reward_credit as i32),
        }
    }
}

pub fn print_mission_list(mut api_mission_list: Vec<APIMission>, entry_limit: Option<usize>) {
    let total_mission_count = api_mission_list.len();

    api_mission_list.sort_by(|a, b| b.begin_timestamp.cmp(&a.begin_timestamp));

    let entry_limit = entry_limit.unwrap_or(api_mission_list.len());

    let mission_list: Vec<MissionTableItem> = api_mission_list
        .into_iter()
        .take(entry_limit)
        .map(|api_mission| api_mission.into())
        .collect();

    println!("Showing {} of total {} missions", mission_list.len(), total_mission_count);

    let mission_list_table = Table::new(&mission_list);

    println!("{}", mission_list_table);
}