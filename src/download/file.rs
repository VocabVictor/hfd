use crate::types::FileInfo;
use crate::config::Config;
use glob::Pattern;

pub fn should_download(config: &Config, file: &FileInfo) -> bool {
    // 如果没有设置任何过滤规则，则下载所有文件
    if config.include_patterns.is_empty() && config.exclude_patterns.is_empty() {
        return true;
    }

    // 如果设置了包含规则，文件必须匹配其中之一
    let mut should_include = config.include_patterns.is_empty();
    if !should_include {
        for pattern in &config.include_patterns {
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
        for pattern in &config.exclude_patterns {
            if let Ok(glob) = Pattern::new(pattern) {
                if glob.matches(&file.rfilename) {
                    return false;
                }
            }
        }
    }

    should_include
} 