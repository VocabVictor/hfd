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
    pub hf_username: Option<String>,
    pub hf_token: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        println!("[DEBUG] Creating default config");
        let config = Self {
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
            hf_username: None,
            hf_token: None,
        };
        println!("[DEBUG] Default config endpoint: {}", config.endpoint);
        config
    }
}

impl Config {
    pub fn load() -> Self {
        println!("[DEBUG] Starting config load");
        // 尝试从多个位置加载配置文件
        let config_paths = vec![
            shellexpand::tilde("~/.hfdconfig").into_owned(),
            "./.hfdconfig".to_string(),
        ];

        println!("[DEBUG] Checking config paths: {:?}", config_paths);

        for path in config_paths {
            println!("[DEBUG] Trying to load config from: {}", path);
            if let Ok(content) = fs::read_to_string(&path) {
                println!("[DEBUG] Successfully read config file content from {}", path);
                println!("[DEBUG] Config file content:\n{}", content);
                
                match toml::from_str::<Config>(&content) {
                    Ok(config) => {
                        println!("[DEBUG] Successfully parsed config from {}", path);
                        println!("[DEBUG] Loaded endpoint: {}", config.endpoint);
                        println!("[DEBUG] Full config: {:?}", config);
                        return config;
                    }
                    Err(e) => {
                        println!("[DEBUG] Failed to parse config from {}: {}", path, e);
                    }
                }
            } else {
                println!("[DEBUG] Failed to read config file: {}", path);
            }
        }

        println!("[DEBUG] No valid config found, using default");
        Self::default()
    }

    pub fn get_model_dir(&self, model_id: &str) -> String {
        println!("[DEBUG] Getting model dir for {}", model_id);
        println!("[DEBUG] Current endpoint: {}", self.endpoint);
        
        if self.use_local_dir {
            let base = shellexpand::tilde(&self.local_dir_base).into_owned();
            let path = PathBuf::from(base).join(model_id);
            let result = path.to_string_lossy().into_owned();
            println!("[DEBUG] Using local dir: {}", result);
            result
        } else {
            let result = format!("models/{}", model_id);
            println!("[DEBUG] Using relative dir: {}", result);
            result
        }
    }
} 