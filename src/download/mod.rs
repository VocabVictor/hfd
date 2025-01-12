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
    pub fn new(config: Arc<Config>) -> Self {
        let concurrent_downloads = config.concurrent_downloads;
        let semaphore = Arc::new(Semaphore::new(concurrent_downloads));
        let multi_progress = Arc::new(MultiProgress::new());
        let progress_bars = Arc::new(Mutex::new(HashMap::new()));
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let active = Arc::new(Mutex::new(HashSet::new()));
        let config = config;

        Self {
            config,
            semaphore,
            multi_progress,
            progress_bars,
            queue,
            active,
        }
    }

    pub fn get_config(&self) -> Arc<Config> {
        self.config.clone()
    }

    pub fn create_progress(&self, filename: String, size: u64) -> Arc<ProgressBar> {
        let mut progress_bars = self.progress_bars.lock().unwrap();
        if let Some(progress) = progress_bars.get(&filename) {
            return progress.clone();
        }

        let progress = Arc::new(self.multi_progress.add(ProgressBar::new(size)));
        progress_bars.insert(filename, progress.clone());
        progress
    }

    pub fn add_to_queue(&self, task: DownloadTask) {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(task);
    }

    pub fn finish_download(&self, filename: &str) {
        let mut active = self.active.lock().unwrap();
        active.remove(filename);
        self.start_next_download();
    }

    fn start_next_download(&self) {
        let mut queue = self.queue.lock().unwrap();
        if let Some(next_task) = queue.pop_front() {
            let mut active = self.active.lock().unwrap();
            active.insert(next_task.filename.clone());
            tokio::spawn(next_task.start());
        }
    }

    pub async fn acquire_permit(&self) -> Option<DownloadTask> {
        let _permit = self.semaphore.acquire().await.ok()?;
        let mut queue = self.queue.lock().unwrap();
        if let Some(task) = queue.pop_front() {
            let mut active = self.active.lock().unwrap();
            active.insert(task.filename.clone());
            Some(task)
        } else {
            None
        }
    }

    pub fn print_status(&self) {
        let queue = self.queue.lock().unwrap();
        let active = self.active.lock().unwrap();
    }
} 