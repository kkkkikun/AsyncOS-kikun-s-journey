use std::fs::File;
use std::io::{self, BufRead, Read};
use std::path::Path;
use anyhow::{Result, Context};
use encoding_rs::{GBK, UTF_8};

/// 学校信息结构
#[derive(Debug, Clone)]
pub struct SchoolInfo {
    pub id: usize,
    pub name: String,
    pub url: String,
}

/// 从CSV文件加载学校URL信息（支持多种编码）
pub fn load_urls_from_csv(csv_path: &str) -> Result<Vec<SchoolInfo>> {
    let path = Path::new(csv_path);

    // 首先读取原始字节
    let mut file = File::open(path)
        .with_context(|| format!("无法打开CSV文件: {}", csv_path))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .context("读取文件内容失败")?;

    // 尝试UTF-8解码
    let (content, encoding) = if let Some(bom) = buffer.get(0..3) {
        // 检查BOM
        if bom == &[0xEF, 0xBB, 0xBF] {
            // UTF-8 with BOM
            let (decoded, _) = UTF_8.decode_with_bom_removal(&buffer);
            (decoded.to_string(), "UTF-8")
        } else {
            // 尝试GBK编码（中文CSV常见）
            let (decoded, _, _) = GBK.decode(&buffer);
            (decoded.to_string(), "GBK")
        }
    } else {
        // 没有BOM，尝试GBK
        let (decoded, _, _) = GBK.decode(&buffer);
        (decoded.to_string(), "GBK")
    };

    println!("📝 检测到文件编码: {}", encoding);

    let mut schools = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        // 跳过空行
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts = parse_csv_line(line);

        // 处理两列格式：院校名称,官方网站
        if parts.len() >= 2 {
            let name = parts[0].trim().to_string();
            let url = parts[1].trim().to_string();

            // 过滤表头和无效数据
            if url.starts_with("http")
                && !name.is_empty()
                && name != "院校名称"
                && name.len() > 1 {
                schools.push(SchoolInfo {
                    id: schools.len() + 1, // 使用序号
                    name,
                    url,
                });
            }
        }
    }

    println!("✅ 成功加载 {} 个学校信息", schools.len());
    Ok(schools)
}

/// 解析CSV行，处理可能的编码问题
fn parse_csv_line(line: &str) -> Vec<String> {
    // 尝试不同的分隔符
    let parts: Vec<&str> = line.split(',').collect();

    // 处理可能的编码问题：如果出现乱码，尝试清理
    parts.into_iter()
        .map(|s| clean_string(s))
        .collect()
}

/// 清理字符串中的乱码和无效字符
fn clean_string(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii() || is_valid_chinese_char(*c))
        .collect()
}

/// 检查是否是有效的中文字符
fn is_valid_chinese_char(c: char) -> bool {
    match c as u32 {
        0x4E00..=0x9FFF => true,  // CJK统一汉字
        0x3400..=0x4DBF => true,  // CJK扩展A
        0x20000..=0x2A6DF => true, // CJK扩展B
        0x2A700..=0x2B73F => true, // CJK扩展C
        0x2B740..=0x2B81F => true, // CJK扩展D
        0x2B820..=0x2CEAF => true, // CJK扩展E
        0xF900..=0xFAFF => true,  // CJK兼容汉字
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_csv_line() {
        let line = "1,北京大学,http://www.pku.edu.cn/";
        let parts = parse_csv_line(line);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "1");
        assert_eq!(parts[1], "北京大学");
        assert_eq!(parts[2], "http://www.pku.edu.cn/");
    }
}
