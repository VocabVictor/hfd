use indicatif::{ProgressBar, ProgressStyle};

pub fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

pub fn create_progress_bar(size: u64, filename: &str) -> ProgressBar {
    let pb = ProgressBar::new(size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes:>9}/{total_bytes:>9} @ {bytes_per_sec:>9} {msg}")
        .unwrap()
        .progress_chars("=>-"));
    pb.set_message(format!("| {:.40}", filename));
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
} 