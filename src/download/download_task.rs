use std::path::PathBuf;
use crate::types::FileInfo;
use super::progress::DownloadProgress;
use std::sync::Arc;

pub enum DownloadTask {
    SmallFile {
        file: FileInfo,
        path: PathBuf,
        progress: Arc<DownloadProgress>,
    },
    ChunkedFile {
        file: FileInfo,
        path: PathBuf,
        chunk_size: usize,
        max_retries: usize,
        progress: Arc<DownloadProgress>,
    },
    Folder {
        name: String,
        files: Vec<FileInfo>,
        base_path: PathBuf,
        progress: Arc<DownloadProgress>,
    },
}

impl DownloadTask {
    pub fn new_folder(name: String, files: Vec<FileInfo>, base_path: PathBuf) -> Self {
        let total_size: u64 = files.iter()
            .filter_map(|f| f.size)
            .sum();
            
        let progress = Arc::new(DownloadProgress::new_folder_progress(&name, total_size));
        
        DownloadTask::Folder {
            name,
            files,
            base_path,
            progress,
        }
    }

    pub fn new_small_file(file: FileInfo, path: PathBuf, folder_name: &str) -> Self {
        let progress = Arc::new(DownloadProgress::new_file_progress(
            &file.rfilename,
            folder_name,
            file.size.unwrap_or(0),
        ));
        
        DownloadTask::SmallFile {
            file,
            path,
            progress,
        }
    }

    pub fn new_chunked_file(file: FileInfo, path: PathBuf, chunk_size: usize, max_retries: usize, folder_name: &str) -> Self {
        let progress = Arc::new(DownloadProgress::new_file_progress(
            &file.rfilename,
            folder_name,
            file.size.unwrap_or(0),
        ));
        
        DownloadTask::ChunkedFile {
            file,
            path,
            chunk_size,
            max_retries,
            progress,
        }
    }

    pub fn get_progress(&self) -> Arc<DownloadProgress> {
        match self {
            DownloadTask::SmallFile { progress, .. } => Arc::clone(progress),
            DownloadTask::ChunkedFile { progress, .. } => Arc::clone(progress),
            DownloadTask::Folder { progress, .. } => Arc::clone(progress),
        }
    }
} 