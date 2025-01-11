use crate::auth::Auth;
use crate::config::Config;
use crate::download::DownloadTask;
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
        
        // 计算总大小
        let total_size: u64 = files.iter()
            .filter_map(|f| f.size)
            .sum();

        println!("Found {} files to download, total size: {:.2} MB", files.len(), total_size as f64 / 1024.0 / 1024.0);

        // 创建下载任务
        let task = DownloadTask::new_folder(
            "".to_string(),
            files,
            base_path.clone(),
            is_dataset,
        );

        // 执行下载
        task.execute(&self.client, self.auth.token.clone(), &self.config.endpoint, model_id).await?;

        Ok(())
    }
} 