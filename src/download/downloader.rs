use crate::auth::Auth;
use crate::config::Config;
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
        let config = Config::new(include_patterns, exclude_patterns)?;
        let auth = Auth::new(token)?;
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
} 