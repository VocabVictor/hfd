use super::downloader::ModelDownloader;
use crate::types::{RepoInfo, FileInfo, RepoFiles};
use pyo3::prelude::*;
use std::path::PathBuf;
use std::fs;
use serde_json::Value;

impl ModelDownloader {
    pub(crate) async fn prepare_download_list(&self, repo_info: &RepoInfo, model_id: &str, base_path: &PathBuf) 
        -> PyResult<(Vec<FileInfo>, u64)> {
        println!("Debug: Starting prepare_download_list");
        
        // 获取文件列表
        let all_files = match &repo_info.files {
            RepoFiles::Model { files } => {
                println!("Debug: Found model repository with {} files", files.len());
                files.clone()
            }
            RepoFiles::Dataset { siblings } => {
                println!("Debug: Found dataset repository with {} files", siblings.len());
                siblings.clone()
            }
        };

        println!("Debug: Total files count: {}", all_files.len());

        // 如果列表为空，可能需要从其他字段获取文件信息
        let all_files = if all_files.is_empty() {
            println!("Debug: No files found, checking tree");
            // 尝试从 extra 中查找可能的文件列表
            if let Some(Value::Array(files)) = repo_info.extra.get("tree") {
                println!("Debug: Found tree with {} entries", files.len());
                let mut tree_files = Vec::new();
                for file in files {
                    if let Some(path) = file.get("path").and_then(|v| v.as_str()) {
                        let size = file.get("size")
                            .and_then(|v| v.as_u64())
                            .or_else(|| file.get("blob_size").and_then(|v| v.as_u64()));
                        tree_files.push(FileInfo {
                            rfilename: path.to_string(),
                            size,
                        });
                    }
                }
                println!("Debug: Added {} files from tree", tree_files.len());
                tree_files
            } else {
                println!("Debug: No tree found in extra");
                println!("Debug: Extra content: {:?}", repo_info.extra);
                Vec::new()
            }
        } else {
            all_files
        };

        let mut files_to_process: Vec<_> = all_files.into_iter()
            .filter(|file| self.should_download_file(&file.rfilename))
            .collect();
        
        println!("Debug: Files after filtering: {}", files_to_process.len());
        if !files_to_process.is_empty() {
            println!("Debug: First file: {}", files_to_process[0].rfilename);
        }

        // 并发获取文件大小
        let mut size_fetch_tasks = Vec::new();
        for file in &files_to_process {
            if file.size.is_none() {
                let file_url = format!(
                    "{}/{}/resolve/main/{}",
                    self.config.endpoint, model_id, file.rfilename
                );
                let client = self.client.clone();
                let token = self.auth.token.clone();
                let filename = file.rfilename.clone();
                
                let task = tokio::spawn(async move {
                    let mut request = client.get(&file_url);
                    if let Some(token) = token {
                        request = request.header("Authorization", format!("Bearer {}", token));
                    }
                    
                    match request.send().await {
                        Ok(response) => {
                            let size = response.content_length();
                            Ok((filename, size))
                        }
                        Err(e) => Err(format!("获取文件大小失败: {}", e))
                    }
                });
                
                size_fetch_tasks.push(task);
            }
        }

        let size_results = futures::future::join_all(size_fetch_tasks).await;
        println!("Debug: Size fetch tasks completed: {}", size_results.len());
        
        // 更新文件大小信息
        for result in size_results {
            if let Ok(Ok((filename, size))) = result {
                if let Some(size) = size {
                    if let Some(file) = files_to_process.iter_mut().find(|f| f.rfilename == filename) {
                        file.size = Some(size);
                    }
                }
            }
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
            }
        }

        println!("Debug: Final files to download: {}", files_to_download.len());
        println!("Debug: Total size to download: {} bytes", total_size);

        // 按文件名排序，并确保没有重复
        files_to_download.sort_by(|a, b| a.rfilename.cmp(&b.rfilename));
        files_to_download.dedup_by(|a, b| a.rfilename == b.rfilename);
        
        Ok((files_to_download, total_size))
    }
} 