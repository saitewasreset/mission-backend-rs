use tabled::Tabled;
use common::admin::APIMissionInvalid;
use common::mission::APIMission;
use crate::formatter::{format_mission_time, format_timestamp_utc};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[derive(Tabled)]
pub struct MissionInvalidTableItem {
    pub mission_id: i32,
    pub begin_time: String,
    pub mission_time: String,
    pub mission_type: String,
    pub reason: String,
}


pub fn print_mission_invalid_list(mut api_mission_invalid_list: Vec<APIMissionInvalid>, api_mission_list: Vec<APIMission>) {
    let mission_id_to_mission_info = api_mission_list
        .into_iter()
        .map(|api_mission| (api_mission.id, api_mission))
        .collect::<std::collections::HashMap<i32, APIMission>>();

    api_mission_invalid_list.sort_by_key(|api_mission_invalid|
        match mission_id_to_mission_info.get(&api_mission_invalid.mission_id) {
            Some(x) => -x.begin_timestamp,
            None => 0,
        });

    let mut mission_invalid_list: Vec<MissionInvalidTableItem> = Vec::with_capacity(api_mission_invalid_list.len());

    for api_mission_invalid in api_mission_invalid_list {
        let mission_info = mission_id_to_mission_info.get(&api_mission_invalid.mission_id);
        let mission_invalid_table_item = match mission_info {
            Some(mission_info) => {
                MissionInvalidTableItem {
                    mission_id: api_mission_invalid.mission_id,
                    begin_time: format_timestamp_utc(mission_info.begin_timestamp),
                    mission_time: format_mission_time(mission_info.mission_time),
                    mission_type: mission_info.mission_type.clone(),
                    reason: api_mission_invalid.reason.clone(),
                }
            }
            None => {
                MissionInvalidTableItem {
                    mission_id: api_mission_invalid.mission_id,
                    begin_time: "?".to_string(),
                    mission_time: "?".to_string(),
                    mission_type: "?".to_string(),
                    reason: api_mission_invalid.reason.clone(),
                }
            }
        };

        mission_invalid_list.push(mission_invalid_table_item);
    }

    println!("Showing {} invalid missions", mission_invalid_list.len());

    let mission_invalid_list_table = tabled::Table::new(&mission_invalid_list);

    println!("{}", mission_invalid_list_table);
}