use crate::auth::Auth;
use crate::config::Config;
use crate::types::{RepoInfo, FileInfo};
use pyo3::prelude::*;
use reqwest::Client;
use std::path::PathBuf;
use super::{download_small_file, download_chunked_file, download_folder, should_download};

pub struct ModelDownloader {
    pub(crate) client: Client,
    pub(crate) config: Config,
    pub(crate) auth: Auth,
    pub(crate) cache_dir: String,
}

impl ModelDownloader {
    pub fn new(
        cache_dir: Option<String>,
        include_patterns: Option<Vec<String>>,
        exclude_patterns: Option<Vec<String>>,
        token: Option<String>,
    ) -> PyResult<Self> {
        let mut config = Config::load()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to load config: {}", e)))?;

        if let Some(include_patterns) = include_patterns {
            config.include_patterns = include_patterns;
        }
        if let Some(exclude_patterns) = exclude_patterns {
            config.exclude_patterns = exclude_patterns;
        }
        
        let auth = Auth { token };
        let cache_dir = cache_dir.unwrap_or_else(|| config.local_dir_base.clone());
        let client = Client::new();

        Ok(Self {
            client,
            config,
            auth,
            cache_dir,
        })
    }

    pub async fn download_files(
        &self,
        model_id: &str,
        base_path: &PathBuf,
        is_dataset: bool,
        repo_info: RepoInfo,
    ) -> PyResult<()> {
        // 从model_id中提取仓库名称（使用斜杠后面的部分）
        let repo_name = model_id.split('/').last().unwrap_or(model_id);
        
        // 使用配置中的路径设置
        let actual_base_path = if self.config.use_local_dir {
            let base = if is_dataset {
                shellexpand::tilde(&self.config.dataset_dir_base).into_owned()
            } else {
                shellexpand::tilde(&self.config.local_dir_base).into_owned()
            };
            PathBuf::from(base).join(repo_name)
        } else {
            base_path.clone()
        };
        
        // 过滤文件列表
        let files: Vec<_> = repo_info.files.into_iter()
            .filter(|file| should_download(&self.config, file))
            .collect();

        // 将文件按文件夹分组
        let mut folder_files: std::collections::HashMap<String, Vec<FileInfo>> = std::collections::HashMap::new();
        let mut root_files: Vec<FileInfo> = Vec::new();

        for file in files {
            if let Some(folder) = file.rfilename.find('/') {
                let folder_name = file.rfilename[..folder].to_string();
                folder_files.entry(folder_name)
                    .or_insert_with(Vec::new)
                    .push(file);
            } else {
                root_files.push(file);
            }
        }

        // 处理根目录下的文件
        for file in root_files {
            let file_path = actual_base_path.join(&file.rfilename);
            
            // 根据文件大小选择下载方式
            if let Some(size) = file.size {
                if size > self.config.parallel_download_threshold {  // 使用配置的阈值
                    download_chunked_file(
                        &self.client,
                        &file,
                        &file_path,
                        self.config.chunk_size,  // 使用配置的块大小
                        self.config.max_retries, // 使用配置的重试次数
                        self.auth.token.clone(),
                        &self.config.endpoint,
                        model_id,
                        is_dataset,
                        None,  // 不使用共享进度条
                    ).await?;
                } else {
                    download_small_file(
                        &self.client,
                        &file,
                        &file_path,
                        self.auth.token.clone(),
                        &self.config.endpoint,
                        model_id,
                        is_dataset,
                        None,  // 不使用共享进度条
                    ).await?;
                }
            } else {
                download_small_file(
                    &self.client,
                    &file,
                    &file_path,
                    self.auth.token.clone(),
                    &self.config.endpoint,
                    model_id,
                    is_dataset,
                    None,  // 不使用共享进度条
                ).await?;
            }
        }

        // 处理文件夹
        for (folder_name, files) in folder_files {
            download_folder(
                self.client.clone(),
                self.config.endpoint.clone(),
                model_id.to_string(),
                actual_base_path.clone(),
                folder_name,
                files,
                self.auth.token.clone(),
                is_dataset,
            ).await?;
        }

        Ok(())
    }
} 