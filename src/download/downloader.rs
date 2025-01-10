use crate::types::{AuthInfo, RepoInfo, RepoFiles};
use crate::config::Config;
use pyo3::prelude::*;
use reqwest::Client;
use tokio::runtime::Runtime;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use ctrlc;

#[pyclass]
pub struct ModelDownloader {
    pub(crate) cache_dir: String,
    pub(crate) client: Client,
    pub(crate) runtime: Runtime,
    pub(crate) config: Config,
    pub(crate) include_patterns: Vec<String>,
    pub(crate) exclude_patterns: Vec<String>,
    pub(crate) running: Arc<AtomicBool>,
    pub(crate) auth: AuthInfo,
    pub(crate) repo_info: Option<RepoInfo>,
}

#[pymethods]
impl ModelDownloader {
    #[new]
    pub fn new(
        cache_dir: Option<String>,
        include_patterns: Option<Vec<String>>,
        exclude_patterns: Option<Vec<String>>,
        token: Option<String>,
    ) -> PyResult<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        
        // 设置 Ctrl+C 处理
        ctrlc::set_handler(move || {
            running_clone.store(false, Ordering::SeqCst);
        }).expect("Error setting Ctrl+C handler");
        
        let config = Config::load();
        
        // 创建优化的 HTTP 客户端
        let client = Client::builder()
            .pool_max_idle_per_host(config.connections_per_download * 2)
            .pool_idle_timeout(std::time::Duration::from_secs(600))
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .tcp_nodelay(true)
            .http2_prior_knowledge()
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(60))
            .connection_verbose(true)
            .use_rustls_tls()
            .http2_keep_alive_interval(std::time::Duration::from_secs(30))
            .http2_keep_alive_timeout(std::time::Duration::from_secs(60))
            .http2_adaptive_window(true)
            .build()
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create HTTP client: {}", e))
            })?;
        
        Ok(Self {
            cache_dir: if let Some(dir) = cache_dir {
                dir
            } else {
                config.get_model_dir("")
            },
            client,
            runtime: Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e))
            })?,
            config,
            include_patterns: include_patterns.unwrap_or_default(),
            exclude_patterns: exclude_patterns.unwrap_or_default(),
            running,
            auth: AuthInfo {
                token,
            },
            repo_info: None,
        })
    }

    pub fn download(&mut self, model_id: &str) -> PyResult<String> {
        self.running.store(true, Ordering::SeqCst);
        let model_id = model_id.to_string();
        let future = async move {
            self.download_model(&model_id).await
        };
        self.runtime.block_on(future)
    }

    pub(crate) fn get_file_url(&self, model_id: &str, filename: &str) -> PyResult<String> {
        if let Some(repo_info) = &self.repo_info {
            let url = match &repo_info.files {
                RepoFiles::Model { .. } => {
                    format!("{}/{}/resolve/main/{}", self.config.endpoint, model_id, filename)
                }
                RepoFiles::Dataset { .. } => {
                    format!("{}/datasets/{}/resolve/main/{}", self.config.endpoint, model_id, filename)
                }
            };
            Ok(url)
        } else {
            Err(pyo3::exceptions::PyRuntimeError::new_err("Repository info not initialized"))
        }
    }
} 