use crate::auth::Auth;
use crate::config::Config;
use crate::download::DownloadTask;
use pyo3::prelude::*;
use reqwest::Client;
use serde_json::Value;

pub struct ModelDownloader {
    pub(crate) client: Client,
    pub(crate) config: Config,
    pub(crate) auth: Auth,
    pub(crate) cache_dir: String,
    pub(crate) repo_info: Option<Value>,
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
            repo_info: None,
        })
    }

    pub async fn download_file(
        &self,
        task: DownloadTask,
        model_id: &str,
    ) -> PyResult<()> {
        task.execute(
            &self.client,
            self.auth.token.clone(),
            &self.config.endpoint,
            model_id,
        ).await
    }

    pub async fn download_folder(
        &self,
        task: DownloadTask,
        model_id: &str,
    ) -> PyResult<()> {
        task.execute(
            &self.client,
            self.auth.token.clone(),
            &self.config.endpoint,
            model_id,
        ).await
    }
} 