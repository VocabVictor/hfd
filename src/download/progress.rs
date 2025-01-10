use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;

#[allow(dead_code)]
pub struct DownloadProgress {
    pub progress_bar: Arc<ProgressBar>,
}

#[allow(dead_code)]
impl DownloadProgress {
    pub fn new_folder_progress(folder_name: &str, total_size: u64) -> Self {
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading folder: {}", folder_name));
        
        Self {
            progress_bar: Arc::new(pb),
        }
    }

    pub fn new_file_progress(file_name: &str, folder_name: &str, size: u64) -> Self {
        let pb = ProgressBar::new(size);
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("[{}] Downloading: {}", folder_name, file_name));
        
        Self {
            progress_bar: Arc::new(pb),
        }
    }

    pub fn update_message(&self, msg: String) {
        self.progress_bar.set_message(msg);
    }

    pub fn inc(&self, n: u64) {
        self.progress_bar.inc(n);
    }

    pub fn finish_download(&self) {
        self.progress_bar.finish_with_message("Download completed");
    }

    pub fn fail_download(&self, msg: &str) {
        self.progress_bar.finish_with_message(format!("Download failed: {}", msg));
    }

    pub fn cancel_download(&self) {
        self.progress_bar.finish_with_message("Download cancelled");
    }
} 