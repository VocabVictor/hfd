use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::{HashMap, VecDeque, HashSet};
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
    active_downloads: Arc<Mutex<HashSet<String>>>,
    semaphore: Arc<Semaphore>,
    config: Arc<Config>,
}

impl DownloadManager {
    pub fn new(config: Arc<Config>) -> Self {
        let concurrent_downloads = config.concurrent_downloads;
        let semaphore = Arc::new(Semaphore::new(concurrent_downloads));
        let multi_progress = Arc::new(MultiProgress::new());
        let file_progress = Arc::new(Mutex::new(HashMap::new()));
        let download_queue = Arc::new(Mutex::new(VecDeque::new()));
        let active_downloads = Arc::new(Mutex::new(HashSet::new()));

        Self {
            config,
            semaphore,
            multi_progress,
            file_progress,
            download_queue,
            active_downloads,
        }
    }

    pub fn get_config(&self) -> Arc<Config> {
        self.config.clone()
    }

    pub fn create_progress(&self, filename: String, size: u64) -> Arc<ProgressBar> {
        let mut file_progress = self.file_progress.lock().unwrap();
        if let Some(progress) = file_progress.get(&filename) {
            return progress.clone();
        }

        let progress = Arc::new(self.multi_progress.add(ProgressBar::new(size)));
        file_progress.insert(filename, progress.clone());
        progress
    }

    pub fn add_to_queue(&self, task: DownloadTask) {
        let mut queue = self.download_queue.lock().unwrap();
        queue.push_back(task);
    }

    pub fn finish_download(&self, filename: &str) {
        let mut active = self.active_downloads.lock().unwrap();
        active.remove(filename);
        self.start_next_download();
    }

    fn start_next_download(&self) {
        let mut queue = self.download_queue.lock().unwrap();
        if let Some(next_task) = queue.pop_front() {
            let mut active = self.active_downloads.lock().unwrap();
            active.insert(next_task.filename.clone());
            tokio::spawn(async move {
                // TODO: Implement actual download logic
            });
        }
    }

    pub async fn acquire_permit(&self) -> Option<DownloadTask> {
        let _permit = self.semaphore.acquire().await.ok()?;
        let mut queue = self.download_queue.lock().unwrap();
        if let Some(task) = queue.pop_front() {
            let mut active = self.active_downloads.lock().unwrap();
            active.insert(task.filename.clone());
            Some(task)
        } else {
            None
        }
    }

    pub fn print_status(&self) {
        let queue = self.download_queue.lock().unwrap();
        let active = self.active_downloads.lock().unwrap();
        // TODO: Implement status printing
    }
} 