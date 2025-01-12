use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::{HashMap, VecDeque};
use tokio::sync::Mutex;
use std::time::Duration;
use crate::config::Config;

pub mod chunk;
pub mod file;
pub mod repo;
pub mod download_task;

#[derive(Clone)]
struct DownloadTask {
    filename: String,
    size: u64,
    progress: Arc<ProgressBar>,
}

#[derive(Clone)]
pub struct DownloadManager {
    multi_progress: Arc<MultiProgress>,
    file_progress: Arc<Mutex<HashMap<String, Arc<ProgressBar>>>>,
    download_queue: Arc<Mutex<VecDeque<DownloadTask>>>,
    active_downloads: Arc<Mutex<HashMap<String, DownloadTask>>>,
    semaphore: Arc<Semaphore>,
    config: Arc<Config>,
}

impl DownloadManager {
    pub fn new(total_size: u64, config: Config) -> Self {
        let multi_progress = Arc::new(MultiProgress::new());
        
        Self {
            multi_progress,
            file_progress: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(Mutex::new(VecDeque::new())),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(config.concurrent_downloads)),
            config: Arc::new(config),
        }
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
        pb.set_message(format!("Waiting to download {}", filename));
        pb.enable_steady_tick(Duration::from_millis(100));

        // 创建下载任务并加入队列
        let task = DownloadTask {
            filename: filename.clone(),
            size,
            progress: pb.clone(),
        };

        let mut queue = self.download_queue.lock().await;
        queue.push_back(task);

        file_progress.insert(filename.clone(), pb.clone());
        pb
    }

    pub async fn update_progress(&self, filename: &str, bytes: u64) {
        let file_progress = self.file_progress.lock().await;
        if let Some(pb) = file_progress.get(filename) {
            pb.inc(bytes);
        }
    }

    pub async fn finish_file(&self, filename: &str) {
        let mut file_progress = self.file_progress.lock().await;
        let mut active_downloads = self.active_downloads.lock().await;
        
        if let Some(pb) = file_progress.remove(filename) {
            pb.finish_with_message(format!("✓ Downloaded {}", filename));
        }
        
        active_downloads.remove(filename);

        // 检查队列中的下一个任务
        let mut queue = self.download_queue.lock().await;
        if let Some(next_task) = queue.pop_front() {
            active_downloads.insert(next_task.filename.clone(), next_task.clone());
            next_task.progress.set_message(format!("Downloading {}", next_task.filename));
        }
    }

    pub async fn acquire_permit(&self) -> tokio::sync::OwnedSemaphorePermit {
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        
        // 获取许可后，从队列中取出任务并开始下载
        let mut queue = self.download_queue.lock().await;
        let mut active_downloads = self.active_downloads.lock().await;
        
        if let Some(task) = queue.pop_front() {
            active_downloads.insert(task.filename.clone(), task.clone());
            task.progress.set_message(format!("Downloading {}", task.filename));
        }
        
        permit
    }

    pub fn get_config(&self) -> Arc<Config> {
        self.config.clone()
    }
} 