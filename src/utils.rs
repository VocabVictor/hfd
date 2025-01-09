use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::sync::OnceLock;
use std::sync::Arc;

static MULTI_PROGRESS: OnceLock<Arc<MultiProgress>> = OnceLock::new();

pub fn create_progress_bar(total_size: u64, prefix: &str, initial: u64) -> ProgressBar {
    let multi = MULTI_PROGRESS.get_or_init(|| Arc::new(MultiProgress::new()));
    let pb = multi.add(ProgressBar::new(total_size));
    
    // 设置更清晰的样式
    pb.set_style(ProgressStyle::with_template(
        "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({percent}%) @ {binary_bytes_per_sec} {msg}",
    )
    .unwrap()
    .progress_chars("=>-"));
    
    pb.set_position(initial);
    pb.set_message(prefix.to_string());
    pb
}

pub fn print_status(msg: &str) -> std::io::Result<()> {
    let multi = MULTI_PROGRESS.get_or_init(|| Arc::new(MultiProgress::new()));
    multi.println(msg)
}

pub fn clear_progress() -> std::io::Result<()> {
    let multi = MULTI_PROGRESS.get_or_init(|| Arc::new(MultiProgress::new()));
    multi.clear()
} 