use tabled::Tabled;
use common::admin::APIMissionInvalid;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[derive(Tabled)]
pub struct MissionInvalidTableItem {
    pub mission_id: i32,
    pub reason: String,
}

impl From<APIMissionInvalid> for MissionInvalidTableItem {
    fn from(api_mission_invalid: APIMissionInvalid) -> Self {
        MissionInvalidTableItem {
            mission_id: api_mission_invalid.mission_id,
            reason: api_mission_invalid.reason,
        }
    }
}

pub fn print_mission_invalid_list(api_mission_invalid_list: Vec<APIMissionInvalid>) {
    let mission_invalid_list: Vec<MissionInvalidTableItem> = api_mission_invalid_list
        .into_iter()
        .map(|api_mission_invalid| api_mission_invalid.into())
        .collect();

    println!("Showing {} invalid missions", mission_invalid_list.len());

    let mission_invalid_list_table = tabled::Table::new(&mission_invalid_list);

    println!("{}", mission_invalid_list_table);
}