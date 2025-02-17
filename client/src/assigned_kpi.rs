use std::collections::HashMap;
use std::str::FromStr;
use tabled::{Table, Tabled};
use common::kpi::{APIAssignedKPI, KPIComponent, PlayerAssignedKPIInfo};
use crate::api::{Authenticated, MissionMonitorClient};
use crate::formatter::format_timestamp_utc;
use crate::kpi::print_player_mission_kpi_info;

#[derive(Copy, Clone, Debug, PartialEq)]
#[derive(Tabled)]
pub struct PlayerAssignedKPIComponentEntry {
    pub component: KPIComponent,
    pub delta_value: f64,
}

pub fn prompt_for_value<T>(prompt: &str, valid_range: Option<&[T]>) -> T
where
    T: PartialEq + FromStr,
{
    loop {
        let input = prompt_for_str::<_, &[&str]>(prompt, None);

        if let Ok(value) = input.parse::<T>() {
            if let Some(valid_range) = valid_range {
                if valid_range.contains(&value) {
                    return value;
                }
            } else {
                return value;
            }
        }

        println!("Invalid value. Please try again.");
    }
}


pub fn prompt_for_str<T, U>(prompt: &str, valid_range: Option<U>) -> String
where
    T: AsRef<str> + PartialEq,
    U: AsRef<[T]>,
{
    let mut input = String::new();

    loop {
        print!("{}", prompt);
        std::io::stdin().read_line(&mut input).unwrap();

        input = input.trim().to_string();

        if let Some(valid_range) = &valid_range {
            for valid_value in valid_range.as_ref() {
                if valid_value.as_ref() == input {
                    return input;
                }
            }
        } else {
            return input;
        }


        println!("Invalid value. Please try again.");
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ComponentReader(Option<KPIComponent>);

impl FromStr for ComponentReader {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match KPIComponent::from_str(s) {
            Ok(x) => Ok(ComponentReader(Some(x))),
            Err(e) => {
                if s.to_lowercase() == "end" {
                    Ok(ComponentReader(None))
                } else {
                    Err(e)
                }
            }
        }
    }
}

pub fn prompt_for_component() -> HashMap<KPIComponent, f64> {
    let mut result = HashMap::new();

    let valid_component = [KPIComponent::Kill,
        KPIComponent::Damage,
        KPIComponent::Priority,
        KPIComponent::Revive,
        KPIComponent::Death,
        KPIComponent::FriendlyFire,
        KPIComponent::Nitra,
        KPIComponent::Supply,
        KPIComponent::Minerals];


    println!("valid component list: {}", valid_component
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(", "));

    println!("Enter 'end' to finish");

    while let Some(target_component) = prompt_for_value::<ComponentReader>("Enter component: ", None).0 {
        let delta_value: f64 = prompt_for_value("Enter delta value: ", None);

        result.insert(target_component, delta_value);
    }

    result
}

pub fn print_assigned_kpi(assigned_kpi: &APIAssignedKPI) {
    println!("Mission ID: {}", assigned_kpi.mission_id);
    println!("Player name: {}", assigned_kpi.player_name);

    let player_assigned_kpi_component_list = assigned_kpi.player_assigned_kpi_info.by_component.iter().map(|(component, delta)| {
        PlayerAssignedKPIComponentEntry {
            component: *component,
            delta_value: *delta,
        }
    }).collect::<Vec<_>>();

    println!("{}", Table::new(&player_assigned_kpi_component_list));

    if let Some(overall) = assigned_kpi.player_assigned_kpi_info.overall {
        println!("Overall: {}", overall);
    }

    if !assigned_kpi.player_assigned_kpi_info.note.is_empty() {
        println!("Note: {}", assigned_kpi.player_assigned_kpi_info.note);
    }
}

pub fn read_assigned_kpi(client: &mut MissionMonitorClient<Authenticated>) -> Result<APIAssignedKPI, String> {
    println!("Getting mission list...");

    let mission_list = Result::from(client.get_api_mission_list()).map_err(|e| format!("cannot get mission list: {}", e))?;

    let mission_id = prompt_for_value("Enter mission ID: ", Some(&mission_list.iter().map(|m| m.id).collect::<Vec<_>>()));

    let selected_mission = &mission_list[mission_id as usize];

    println!("mission_id = {} timestamp = {}", selected_mission.id, format_timestamp_utc(selected_mission.begin_timestamp));

    println!("Getting mission info...");

    let selected_mission_info = Result::from(client.get_mission_general_info(selected_mission.id)).map_err(|e| format!("cannot get mission info: {}", e))?;

    println!("Player list for mission {}:", mission_id);

    let valid_player_name_list = selected_mission_info.player_info.keys().collect::<Vec<_>>();

    for (player_name, player_data) in &selected_mission_info.player_info {
        println!("name = {}, character = {}", player_name, player_data.character_game_id);
    }

    let target_player_name = prompt_for_str("Enter player name: ", Some(&valid_player_name_list));

    println!("Getting player kpi info...");

    let mission_kpi_info = Result::from(client.get_mission_kpi_info(mission_id)).map_err(|e| format!("cannot get mission kpi info: {}", e))?;

    let player_kpi_info = mission_kpi_info
        .into_iter()
        .find(|info| info.player_name == target_player_name)
        .unwrap();

    print_player_mission_kpi_info(player_kpi_info);

    let component_map = prompt_for_component();

    let overall_delta_input: f64 = prompt_for_value("Enter overall delta value, '0' to skip: ", None);

    let note_input = prompt_for_str::<_, &[&str]>("Enter note, empty line to skip: ", None);

    let overall_delta = if overall_delta_input == 0.0 {
        None
    } else {
        Some(overall_delta_input)
    };

    let note_input = if note_input.is_empty() {
        None
    } else {
        Some(note_input)
    };

    Ok(APIAssignedKPI {
        mission_id,
        player_name: target_player_name,
        player_assigned_kpi_info: PlayerAssignedKPIInfo {
            by_component: component_map,
            overall: overall_delta,
            note: note_input.unwrap_or_default(),
        },
    })
}
