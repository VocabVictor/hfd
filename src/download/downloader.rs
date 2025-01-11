use crate::auth::Auth;
use crate::config::Config;
use crate::download::DownloadTask;
use crate::types::RepoInfo;
use pyo3::prelude::*;
use reqwest::Client;
use std::path::PathBuf;

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
        let mut config = Config::load();
        if let Some(include_patterns) = include_patterns {
            config.include_patterns = include_patterns;
        }
        if let Some(exclude_patterns) = exclude_patterns {
            config.exclude_patterns = exclude_patterns;
        }
        
        let auth = Auth { token };
        let cache_dir = cache_dir.unwrap_or_else(|| "./.cache".to_string());
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
        println!("[DEBUG] Creating endpoint: {}", self.config.endpoint);
        
        // 过滤文件列表
        let files: Vec<_> = repo_info.files.into_iter()
            .filter(|file| super::file_filter::should_download(&self.config, file))
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
            let file_path = base_path.join(&file.rfilename);
            
            // 根据文件大小选择下载方式
            let task = if let Some(size) = file.size {
                if size > 10 * 1024 * 1024 {  // 大于10MB的文件使用分块下载
                    DownloadTask::large_file(
                        file,
                        file_path,
                        1024 * 1024,  // 1MB chunks
                        3,
                        "",
                        is_dataset,
                    )
                } else {
                    DownloadTask::single_file(
                        file,
                        file_path,
                        "",
                        is_dataset,
                    )
                }
            } else {
                DownloadTask::single_file(
                    file,
                    file_path,
                    "",
                    is_dataset,
                )
            };

            task.execute(&self.client, self.auth.token.clone(), &self.config.endpoint, model_id).await?;
        }

        // 处理文件夹
        for (folder_name, files) in folder_files {
            let task = DownloadTask::folder(
                folder_name,
                files,
                base_path.clone(),
                is_dataset,
            );

            task.execute(&self.client, self.auth.token.clone(), &self.config.endpoint, model_id).await?;
        }

        Ok(())
    }
} 