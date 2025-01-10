use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub endpoint: String,
    pub use_local_dir: bool,
    pub local_dir_base: String,
    pub concurrent_downloads: usize,
    pub max_download_speed: Option<u64>,
    pub connections_per_download: usize,
    pub parallel_download_threshold: u64,
    pub buffer_size: usize,
    pub chunk_size: usize,
    pub max_retries: usize,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: "https://huggingface.co".to_string(),
            use_local_dir: false,
            local_dir_base: "~/.cache/huggingface/hub".to_string(),
            concurrent_downloads: 4,
            max_download_speed: None,
            connections_per_download: 8,
            parallel_download_threshold: 50 * 1024 * 1024, // 50MB
            buffer_size: 8 * 1024 * 1024, // 8MB
            chunk_size: 16 * 1024 * 1024, // 16MB
            max_retries: 3,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        // 尝试从多个位置加载配置文件
        let config_paths = vec![
            shellexpand::tilde("~/.hfdconfig").into_owned(),
            "./.hfdconfig".to_string(),
        ];

        for path in config_paths {
            if let Ok(content) = fs::read_to_string(&path) {
                println!("Loading config from {}", path);
                if let Ok(config) = toml::from_str::<Config>(&content) {
                    println!("Using endpoint: {}", config.endpoint);
                    return config;
                }
            }
        }

        println!("No config file found, using default settings");
        Self::default()
    }

    pub fn get_model_dir(&self, model_id: &str) -> String {
        if self.use_local_dir {
            let base = shellexpand::tilde(&self.local_dir_base).into_owned();
            let path = PathBuf::from(base).join(model_id);
            path.to_string_lossy().into_owned()
        } else {
            format!("models/{}", model_id)
        }
    }
} 