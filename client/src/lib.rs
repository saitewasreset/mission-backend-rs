pub mod load;
pub mod api;

pub fn format_size(size: usize) -> String {
    match size {
        0..1024 => format!("{}B", size),
        1024..1048576 => format!("{:.2}KiB", size as f64 / 1024.0),
        1048576.. => format!("{:.2}MiB", size as f64 / (1024.0 * 1024.0)),
    }
}

