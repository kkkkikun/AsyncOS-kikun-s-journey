use std::fs;
use std::path::Path;
use anyhow::{Result, Context};

/// 保存爬取的文本内容到文件
pub fn save_text_to_file(content: &str, filename: &str, output_dir: &str) -> Result<String> {
    // 确保输出目录存在
    fs::create_dir_all(output_dir)
        .with_context(|| format!("无法创建输出目录: {}", output_dir))?;

    // 清理文件名：移除不安全的字符
    let safe_filename = sanitize_filename(filename);

    // 构建完整路径
    let file_path = Path::new(output_dir).join(format!("{}.txt", safe_filename));

    // 写入文件
    fs::write(&file_path, content)
        .with_context(|| format!("无法写入文件: {:?}", file_path))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// 清理文件名，移除不安全的字符
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            match c {
                // 保留中文字符、字母、数字和基本符号
                c if c.is_alphanumeric() => c,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// 读取保存的文件内容（用于验证）
pub fn read_saved_file(filename: &str, output_dir: &str) -> Result<String> {
    let safe_filename = sanitize_filename(filename);
    let file_path = Path::new(output_dir).join(format!("{}.txt", safe_filename));

    fs::read_to_string(&file_path)
        .with_context(|| format!("无法读取文件: {:?}", file_path))
}

/// 列出所有已保存的文件
pub fn list_saved_files(output_dir: &str) -> Result<Vec<String>> {
    let path = Path::new(output_dir);

    if !path.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(path)
        .with_context(|| format!("无法读取目录: {}", output_dir))?;

    let files: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "txt"))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("北京大学"), "北京大学");
        assert_eq!(sanitize_filename("test/file"), "test_file");
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
    }
}
