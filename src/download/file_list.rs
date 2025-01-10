use super::downloader::ModelDownloader;
use crate::types::{RepoInfo, FileInfo};
use pyo3::prelude::*;
use std::path::PathBuf;
use std::fs;

impl ModelDownloader {
    async fn get_file_size_from_resolve(&self, model_id: &str, filename: &str, repo_info: &RepoInfo) -> PyResult<Option<u64>> {
        let file_url = if repo_info.is_dataset() {
            format!(
                "{}/datasets/{}/resolve/main/{}",
                self.config.endpoint, model_id, filename
            )
        } else {
            format!(
                "{}/{}/resolve/main/{}",
                self.config.endpoint, model_id, filename
            )
        };
        
        println!("Debug: Getting size for file: {} from {}", filename, file_url);
        
        let mut request = self.client.head(&file_url);
        if let Some(token) = &self.auth.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        match request.send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    println!("Debug: Failed to get size for {}: {}", filename, response.status());
                    return Ok(None);
                }
                
                // 首先尝试从 x-linked-size 获取大小
                if let Some(size) = response.headers()
                    .get("x-linked-size")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok()) 
                {
                    println!("Debug: Got size from x-linked-size for {}: {}", filename, size);
                    return Ok(Some(size));
                }
                
                // 如果没有 x-linked-size，则使用 content-length
                let size = response.content_length();
                println!("Debug: Got size from content-length for {}: {:?}", filename, size);
                Ok(size)
            }
            Err(e) => {
                println!("Debug: Error getting size for {}: {}", filename, e);
                Ok(None)
            }
        }
    }

    pub(crate) async fn prepare_download_list(&self, repo_info: &RepoInfo, model_id: &str, base_path: &PathBuf) 
        -> PyResult<(Vec<FileInfo>, u64)> {
        println!("Debug: Starting prepare_download_list for {} repository", 
            if repo_info.is_dataset() { "dataset" } else { "model" });
        
        // 获取文件列表
        let mut all_files = Vec::new();
        for file in &repo_info.siblings {
            if let Some(size) = self.get_file_size_from_resolve(model_id, &file.rfilename, repo_info).await? {
                all_files.push(FileInfo {
                    rfilename: file.rfilename.clone(),
                    size: Some(size),
                });
            } else {
                all_files.push(FileInfo {
                    rfilename: file.rfilename.clone(),
                    size: None,
                });
            }
        }

        println!("Debug: Total files count: {}", all_files.len());

        let mut files_to_process: Vec<_> = all_files.into_iter()
            .filter(|file| self.should_download_file(&file.rfilename))
            .collect();
        
        println!("Debug: Files after filtering: {}", files_to_process.len());
        if !files_to_process.is_empty() {
            println!("Debug: First file: {}", files_to_process[0].rfilename);
        }

        let mut files_to_download = Vec::new();
        let mut total_size: u64 = 0;

        for file in files_to_process {
            let file_path = base_path.join(&file.rfilename);
            
            // 检查文件是否已下载
            if let Ok(metadata) = fs::metadata(&file_path) {
                if let Some(expected_size) = file.size {
                    if metadata.len() == expected_size {
                        println!("Debug: Skipping already downloaded file: {}", file.rfilename);
                        continue;  // 跳过已下载完成的文件
                    }
                }
            }

            if let Some(size) = file.size {
                total_size += size;
                files_to_download.push(file);
            } else {
                println!("Debug: No size information for file: {}", file.rfilename);
                files_to_download.push(file);
            }
        }

        println!("Debug: Final files to download: {}", files_to_download.len());
        println!("Debug: Total size to download: {} bytes", total_size);

        // 按文件名排序，并确保没有重复
        files_to_download.sort_by(|a, b| a.rfilename.cmp(&b.rfilename));
        
        Ok((files_to_download, total_size))
    }
} 