use common::cache::{APICacheStatus, APICacheStatusItem};
use chrono::{TimeZone, Utc};
use tabled::{Table, Tabled};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[derive(Tabled)]
struct CacheStatusTableItem {
    pub cache_type: String,
    pub last_update: String,
    pub success: bool,
    pub last_success_time_info: String,
    pub last_error_message: String,
}

impl From<APICacheStatusItem> for CacheStatusTableItem {
    fn from(item: APICacheStatusItem) -> Self {
        let last_update_str = Utc.timestamp_opt(item.last_update, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "N/A".to_string());

        CacheStatusTableItem {
            cache_type: item.cache_type,
            last_update: last_update_str,
            success: item.last_success,
            last_success_time_info: format_time_info(item.last_success_data),
            last_error_message: item.last_error_message,
        }
    }
}

pub fn print_cache_status(cache_status: APICacheStatus) {
    println!("Current working: {}", cache_status.working);

    let cache_status_table_items: Vec<CacheStatusTableItem> = cache_status.items
        .into_iter()
        .map(|item| item.into())
        .collect();

    let cache_item_table = Table::new(&cache_status_table_items);
    println!("{}", cache_item_table);
}

fn format_time_info(time_info: (i64, f64, f64)) -> String {
    format!("{}ms + {}ms", time_info.1 as i64, time_info.2 as i64)
}