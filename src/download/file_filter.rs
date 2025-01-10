use super::downloader::ModelDownloader;
use regex;

impl ModelDownloader {
    pub(crate) fn should_download_file(&self, filename: &str) -> bool {
        // 如果没有指定包含模式，则默认包含所有文件
        let mut should_include = self.include_patterns.is_empty();
        
        // 检查文件是否匹配任何包含模式
        for pattern in &self.include_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(filename) {
                    should_include = true;
                    break;
                }
            }
        }
        
        // 如果文件不应该被包含，直接返回 false
        if !should_include {
            return false;
        }
        
        // 检查文件是否匹配任何排除模式
        for pattern in &self.exclude_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(filename) {
                    return false;
                }
            }
        }
        
        true
    }
} 