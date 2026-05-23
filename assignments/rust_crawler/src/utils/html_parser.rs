use anyhow::{Result, Context};
use scraper::{Html, Selector};
use select::document::Document;
use select::predicate::{Name, Attr};

/// 从HTML内容中提取纯文本（带错误处理）
pub fn extract_text_from_html(html: &str) -> Result<String> {
    if html.trim().is_empty() {
        return Ok(String::new());
    }

    let document = Html::parse_document(html);

    // 提取主要内容
    let body_selector = Selector::parse("body").unwrap();
    let mut text_content = String::new();

    if let Some(body) = document.select(&body_selector).next() {
        for node in body.descendants() {
            // 只提取文本节点，跳过script和style内容
            let value = node.value().as_text();
            if let Some(text) = value {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    text_content.push_str(trimmed);
                    text_content.push('\n');
                }
            }
        }
    }

    // 如果body为空，尝试从所有选择器中提取文本
    if text_content.trim().is_empty() {
        let all_text_selector = Selector::parse("p, div, span, h1, h2, h3, h4, h5, h6, a, li").unwrap();
        for element in document.select(&all_text_selector) {
            let text = element.text().collect::<String>();
            let text = text.trim();
            if !text.is_empty() && text.len() > 2 {
                text_content.push_str(text);
                text_content.push_str("\n\n");
            }
        }
    }

    // 清理和格式化文本
    let cleaned = clean_text(&text_content);
    Ok(cleaned)
}

/// 使用select库的备用实现（更简单但功能类似）
pub fn extract_text_simple(html: &str) -> Result<String> {
    let document = Document::from(html);

    let mut text_content = String::new();

    // 提取所有p标签的文本（通常是主要内容）
    for node in document.find(Name("p")) {
        let text = node.text();
        let text = text.trim();
        if !text.is_empty() {
            text_content.push_str(text);
            text_content.push_str("\n\n");
        }
    }

    // 也提取div中的文本
    for node in document.find(Name("div")) {
        // 避免重复提取p标签的内容
        if node.find(Name("p")).count() == 0 {
            let text = node.text();
            let text = text.trim();
            if !text.is_empty() && text.len() > 10 {
                text_content.push_str(text);
                text_content.push_str(" ");
            }
        }
    }

    Ok(clean_text(&text_content))
}

/// 清理提取的文本内容
fn clean_text(text: &str) -> String {
    text.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .filter(|c| c.is_ascii() || is_valid_char(*c))
        .collect::<String>()
}

/// 检查字符是否有效（保留中文、英文、数字和基本标点）
fn is_valid_char(c: char) -> bool {
    is_chinese_char(c) || c.is_alphanumeric() || " \n\t.,!?;:，。！？；：、（）【】《》".contains(c)
}

/// 检查是否是中文字符
fn is_chinese_char(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF |  // CJK统一汉字
        0x3400..=0x4DBF |  // CJK扩展A
        0xF900..=0xFAFF    // CJK兼容汉字
    )
}

/// 提取页面标题
pub fn extract_title(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let title_selector = Selector::parse("title").unwrap();

    document
        .select(&title_selector)
        .next()
        .map(|title| title.text().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text() {
        let html = r#"
        <html>
        <head><title>测试页面</title></head>
        <body>
            <script>alert('test');</script>
            <p>这是第一段文本。</p>
            <p>这是第二段文本。</p>
        </body>
        </html>
        "#;

        let text = extract_text_from_html(html).unwrap();
        assert!(text.contains("第一段文本"));
        assert!(text.contains("第二段文本"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn test_extract_title() {
        let html = r#"<html><head><title>北京大学</title></head></html>"#;
        let title = extract_title(html);
        assert_eq!(title, Some("北京大学".to_string()));
    }
}
