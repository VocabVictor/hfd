use std::sync::Arc;
use tokio::sync::Semaphore;
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
    #[allow(dead_code)]
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
    is_folder: bool,  // 是否是文件夹下载
    folder_progress: Arc<Mutex<Option<Arc<ProgressBar>>>>,  // 文件夹总进度条
}

impl DownloadManager {
    pub fn new(_total_size: u64, config: Config) -> Self {
        let multi_progress = Arc::new(MultiProgress::new());
        
        Self {
            multi_progress,
            file_progress: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(Mutex::new(VecDeque::new())),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(config.concurrent_downloads)),
            config: Arc::new(config),
            is_folder: false,
            folder_progress: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_folder(total_size: u64, folder_name: String, config: Config) -> Self {
        let multi_progress = Arc::new(MultiProgress::new());
        let pb = Arc::new(multi_progress.add(ProgressBar::new(total_size)));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading folder {}", folder_name));
        pb.enable_steady_tick(Duration::from_millis(100));
        
        Self {
            multi_progress,
            file_progress: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(Mutex::new(VecDeque::new())),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(Semaphore::new(config.concurrent_downloads)),
            config: Arc::new(config),
            is_folder: true,
            folder_progress: Arc::new(Mutex::new(Some(pb))),
        }
    }

    pub async fn get_progress(&self, _filename: &str) -> Arc<ProgressBar> {
        // 如果是文件夹下载，返回文件夹进度条
        if self.is_folder {
            let folder_progress = self.folder_progress.lock().await;
            return folder_progress.as_ref()
                .map(|pb| pb.clone())
                .expect("Folder progress bar not found in folder download mode");
        }

        // 对于单文件下载，返回对应的进度条
        let file_progress = self.file_progress.lock().await;
        file_progress.get(_filename)
            .map(|pb| pb.clone())
            .expect(&format!("Progress bar not found for file: {}", _filename))
    }

    pub async fn create_file_progress(&self, filename: String, size: u64) -> Arc<ProgressBar> {
        // 如果是文件夹下载，返回文件夹进度条
        if self.is_folder {
            let folder_progress = self.folder_progress.lock().await;
            if let Some(pb) = folder_progress.as_ref() {
                return pb.clone();
            }
            // 如果没有找到文件夹进度条，这是一个错误状态
            panic!("Folder progress bar not found in folder download mode");
        }

        let mut file_progress = self.file_progress.lock().await;
        
        // 如果进度条已经存在，先移除它
        if let Some(old_pb) = file_progress.remove(&filename) {
            old_pb.finish_and_clear();
        }

        let pb = Arc::new(self.multi_progress.add(ProgressBar::new(size)));
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
            .unwrap()
            .progress_chars("#>-"));
        pb.set_message(format!("Downloading {}", filename));
        pb.enable_steady_tick(Duration::from_millis(100));

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
        if self.is_folder {
            // 如果是文件夹下载，只更新文件夹总进度条
            let folder_progress = self.folder_progress.lock().await;
            if let Some(pb) = folder_progress.as_ref() {
                pb.inc(bytes);
            }
            return;
        }

        let file_progress = self.file_progress.lock().await;
        if let Some(pb) = file_progress.get(filename) {
            pb.inc(bytes);
            pb.set_message(format!("Downloading {}", filename));
        }
    }

    pub async fn finish_file(&self, filename: &str) {
        if self.is_folder {
            // 如果是文件夹下载，不处理单个文件的完成
            return;
        }

        let mut file_progress = self.file_progress.lock().await;
        let mut active_downloads = self.active_downloads.lock().await;
        
        if let Some(pb) = file_progress.remove(filename) {
            pb.finish_with_message(format!("✓ Downloaded {}", filename));
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.green/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
                .unwrap()
                .progress_chars("#>-"));
        }
        
        active_downloads.remove(filename);

        let mut queue = self.download_queue.lock().await;
        if let Some(next_task) = queue.pop_front() {
            active_downloads.insert(next_task.filename.clone(), next_task.clone());
            next_task.progress.set_message(format!("Downloading {}", next_task.filename));
        }
    }

    pub async fn finish_folder(&self) {
        if !self.is_folder {
            return;
        }

        let folder_progress = self.folder_progress.lock().await;
        if let Some(pb) = folder_progress.as_ref() {
            pb.finish_with_message("✓ Folder download completed");
            pb.set_style(ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.green/blue}] {bytes}/{total_bytes} ({binary_bytes_per_sec}) {msg}")
                .unwrap()
                .progress_chars("#>-"));
        }
    }

    pub async fn cleanup(&self) {
        if self.is_folder {
            // 如果是文件夹下载，只清理文件夹进度条
            let mut folder_progress = self.folder_progress.lock().await;
            if let Some(pb) = folder_progress.take() {
                pb.finish_and_clear();
            }
        } else {
            // 清理所有单文件进度条
            let mut file_progress = self.file_progress.lock().await;
            for (_, pb) in file_progress.drain() {
                pb.finish_and_clear();
            }
        }

        // 清理下载队列和活动下载
        let mut queue = self.download_queue.lock().await;
        queue.clear();
        let mut active_downloads = self.active_downloads.lock().await;
        active_downloads.clear();
    }

    pub async fn handle_interrupt(&self, _filename: &str) {
        // 在文件夹下载模式下，所有中断都通过文件夹进度条处理
        if self.is_folder {
            let folder_progress = self.folder_progress.lock().await;
            if let Some(pb) = folder_progress.as_ref() {
                pb.abandon_with_message("⚠ Download interrupted");
            }
            self.cleanup().await;
            return;
        }

        // 处理单文件进度条
        let file_progress = self.file_progress.lock().await;
        if let Some(pb) = file_progress.get(_filename) {
            pb.abandon_with_message(format!("⚠ Interrupted: {}", _filename));
        }
    }

    pub async fn handle_folder_interrupt(&self) {
        // 确保在文件夹下载模式下
        if !self.is_folder {
            return;
        }

        // 处理文件夹进度条
        let folder_progress = self.folder_progress.lock().await;
        if let Some(pb) = folder_progress.as_ref() {
            pb.abandon_with_message("⚠ Download interrupted");
        }

        // 清理所有资源
        drop(folder_progress); // 先释放锁，避免死锁
        self.cleanup().await;
    }

    pub fn get_config(&self) -> Arc<Config> {
        self.config.clone()
    }
} 