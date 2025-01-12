pub mod downloader;
mod file;
pub mod repo;
mod download_task;
mod chunk;

pub use file::should_download;
pub use download_task::{download_small_file, download_folder};
pub use chunk::download_chunked_file; 