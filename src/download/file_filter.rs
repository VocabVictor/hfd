use super::downloader::ModelDownloader;
use crate::types::FileInfo;
use glob::Pattern;

impl ModelDownloader {
    pub(crate) fn should_download(&self, file: &FileInfo) -> bool {
        // 如果没有设置任何过滤规则，则下载所有文件
        if self.config.include_patterns.is_empty() && self.config.exclude_patterns.is_empty() {
            return true;
        }

        // 如果设置了包含规则，文件必须匹配其中之一
        let mut should_include = self.config.include_patterns.is_empty();
        if !should_include {
            for pattern in &self.config.include_patterns {
                if let Ok(glob) = Pattern::new(pattern) {
                    if glob.matches(&file.rfilename) {
                        should_include = true;
                        break;
                    }
                }
            }
        }

        // 如果文件匹配任何排除规则，则不下载
        if should_include {
            for pattern in &self.config.exclude_patterns {
                if let Ok(glob) = Pattern::new(pattern) {
                    if glob.matches(&file.rfilename) {
                        return false;
                    }
                }
            }
        }

        should_include
    }
} 