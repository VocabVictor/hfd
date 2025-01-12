use std::sync::Arc;
use tokio::sync::Semaphore;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::time::Duration;
use crate::config::Config;

pub mod chunk;
pub mod file;
pub mod repo;
pub mod download_task;

#[derive(Clone)]
pub struct DownloadManager {
    multi_progress: Arc<MultiProgress>,
    total_progress: Arc<ProgressBar>,
    file_progress: Arc<Mutex<HashMap<String, Arc<ProgressBar>>>>,
    semaphore: Arc<Semaphore>,
    config: Arc<Config>,
}

impl DownloadManager {
    pub fn new(total_size: u64, config: Config) -> Self {
        let multi_progress = Arc::new(MultiProgress::new());
        
        // 创建总进度条
        let total_progress = Arc::new(multi_progress.add(ProgressBar::new(total_size)));
        total_progress.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        total_progress.set_message("Downloading files");
        total_progress.enable_steady_tick(Duration::from_millis(100));

        Self {
            multi_progress,
            total_progress,
            file_progress: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(config.concurrent_downloads)),
            config: Arc::new(config),
        }
    }

    pub fn get_total_progress(&self) -> Arc<ProgressBar> {
        self.total_progress.clone()
    }

    pub async fn create_file_progress(&self, filename: String, size: u64) -> Arc<ProgressBar> {
        let mut file_progress = self.file_progress.lock().await;
        if let Some(pb) = file_progress.get(&filename) {
            return pb.clone();
        }

        let pb = Arc::new(self.multi_progress.add(ProgressBar::new(size)));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading {}", filename));
        pb.enable_steady_tick(Duration::from_millis(100));

        file_progress.insert(filename.clone(), pb.clone());
        pb
    }

    pub async fn update_progress(&self, filename: &str, bytes: u64) {
        let file_progress = self.file_progress.lock().await;
        if let Some(pb) = file_progress.get(filename) {
            pb.inc(bytes);
            self.total_progress.inc(bytes);
        }
    }

    pub async fn finish_file(&self, filename: &str) {
        let mut file_progress = self.file_progress.lock().await;
        if let Some(pb) = file_progress.remove(filename) {
            pb.finish_with_message(format!("✓ Downloaded {}", filename));
        }
    }

    pub async fn acquire_permit(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.semaphore.clone().acquire_owned().await.unwrap()
    }

    pub fn get_config(&self) -> Arc<Config> {
        self.config.clone()
    }
}

// 只导出实际使用的内容
pub use download_task::download_folder; 