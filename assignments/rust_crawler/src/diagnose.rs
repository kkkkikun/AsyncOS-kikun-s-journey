use std::time::Duration;
use anyhow::Result;

/// 单个网站诊断工具
pub struct SiteDiagnoser {
    url: String,
    name: String,
}

impl SiteDiagnoser {
    pub fn new(name: String, url: String) -> Self {
        Self { name, url }
    }

    /// 运行完整的诊断
    pub fn diagnose(&self) -> Result<DiagnosisReport> {
        println!("\n🔍 诊断网站: {} ({})", self.name, self.url);
        println!("{}", "─".repeat(60));

        let mut report = DiagnosisReport {
            name: self.name.clone(),
            url: self.url.clone(),
            ..Default::default()
        };

        // 1. DNS解析测试
        println!("📡 1. DNS解析测试...");
        if let Ok(host) = self.test_dns() {
            println!("   ✅ DNS解析成功: {}", host);
            report.dns_ok = true;
        } else {
            println!("   ❌ DNS解析失败");
        }

        // 2. TCP连接测试
        println!("🔌 2. TCP连接测试...");
        match self.test_tcp_connect() {
            Ok(duration) => {
                println!("   ✅ TCP连接成功: {:?}", duration);
                report.tcp_ok = true;
                report.tcp_duration = Some(duration);
            }
            Err(e) => {
                println!("   ❌ TCP连接失败: {}", e);
            }
        }

        // 3. HTTP请求测试（无headers）
        println!("🌐 3. HTTP请求测试（无headers）...");
        match self.test_http_no_headers() {
            Ok((status, duration)) => {
                println!("   ✅ HTTP请求成功: {} (耗时: {:?})", status, duration);
                report.http_no_headers = Some((status.to_string(), duration));
            }
            Err(e) => {
                println!("   ❌ HTTP请求失败: {}", e);
            }
        }

        // 4. HTTP请求测试（完整headers）
        println!("🌐 4. HTTP请求测试（完整headers）...");
        match self.test_http_with_headers() {
            Ok((status, duration, headers_len)) => {
                println!("   ✅ HTTP请求成功: {} (耗时: {:?})", status, duration);
                println!("   📋 响应Headers数量: {}", headers_len);
                report.http_with_headers = Some((status.to_string(), duration));
            }
            Err(e) => {
                println!("   ❌ HTTP请求失败: {}", e);
            }
        }

        // 5. SSL/TLS测试
        println!("🔒 5. SSL/TLS测试...");
        match self.test_ssl() {
            Ok(()) => {
                println!("   ✅ SSL/TLS握手成功");
                report.ssl_ok = true;
            }
            Err(e) => {
                println!("   ❌ SSL/TLS测试失败: {}", e);
            }
        }

        // 6. 内容大小测试
        println!("📄 6. 响应内容大小测试...");
        match self.test_content_size() {
            Ok(size) => {
                println!("   ✅ 响应大小: {} bytes", size);
                report.content_size = Some(size);
            }
            Err(e) => {
                println!("   ❌ 内容大小测试失败: {}", e);
            }
        }

        println!("{}", "─".repeat(60));

        // 7. 总结和建议
        println!("📋 诊断总结:");
        self.print_summary(&report);

        Ok(report)
    }

    fn test_dns(&self) -> Result<String> {
        let url_parts: Vec<&str> = self.url.split("://").collect();
        if url_parts.len() < 2 {
            return Ok(String::new());
        }
        let host = url_parts[1].split('/').next().unwrap_or("");
        Ok(host.to_string())
    }

    fn test_tcp_connect(&self) -> Result<Duration> {
        use std::net::TcpListener;

        let url_parts: Vec<&str> = self.url.split("://").collect();
        if url_parts.len() < 2 {
            anyhow::bail!("无效的URL");
        }

        let host = url_parts[1].split('/').next().unwrap_or("");
        let port = if self.url.starts_with("https") { 443 } else { 80 };

        let start = std::time::Instant::now();
        let _stream = std::net::TcpStream::connect((host, port))?;
        let duration = start.elapsed();

        Ok(duration)
    }

    fn test_http_no_headers(&self) -> Result<(reqwest::StatusCode, Duration)> {
        use reqwest::blocking::Client;

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let start = std::time::Instant::now();
        let response = client.get(&self.url).send()?;
        let status = response.status();
        let duration = start.elapsed();

        Ok((status, duration))
    }

    fn test_http_with_headers(&self) -> Result<(reqwest::StatusCode, Duration, usize)> {
        use crate::utils::{create_default_headers, create_blocking_client_builder};

        let client = create_blocking_client_builder(Duration::from_secs(10))
            .build()?;

        let start = std::time::Instant::now();
        let response = client.get(&self.url).send()?;
        let status = response.status();
        let headers_count = response.headers().len();
        let duration = start.elapsed();

        Ok((status, duration, headers_count))
    }

    fn test_ssl(&self) -> Result<()> {
        if !self.url.starts_with("https") {
            return Ok(()); // 不是HTTPS，跳过SSL测试
        }

        use reqwest::blocking::Client;
        use crate::utils::create_blocking_client_builder;

        let client = create_blocking_client_builder(Duration::from_secs(10))
            .build()?;

        let response = client.get(&self.url).send()?;
        let _ = response.status();

        Ok(())
    }

    fn test_content_size(&self) -> Result<usize> {
        use crate::utils::{create_blocking_client_builder, html_parser};

        let client = create_blocking_client_builder(Duration::from_secs(10))
            .build()?;

        let response = client.get(&self.url).send()?;
        let html = response.text()?;
        let text = html_parser::extract_text_from_html(&html)?;

        Ok(text.len())
    }

    fn print_summary(&self, report: &DiagnosisReport) {
        let issues = vec![
            (!report.dns_ok, "DNS解析问题"),
            (!report.tcp_ok, "TCP连接问题"),
            (report.http_no_headers.is_none(), "HTTP请求失败"),
            (report.http_with_headers.is_none(), "带headers的HTTP请求失败"),
            (!report.ssl_ok && self.url.starts_with("https"), "SSL/TLS问题"),
        ];

        let problem_issues: Vec<_> = issues.into_iter().filter(|(has_issue, _)| *has_issue).collect();

        if problem_issues.is_empty() {
            println!("   ✅ 网站响应正常，可能是临时性问题");
        } else {
            println!("   ❌ 发现的问题:");
            for (_, issue) in problem_issues {
                println!("      - {}", issue);
            }
        }

        // 给出具体建议
        println!("\n💡 建议:");
        if self.url.contains("nuaa.edu.cn") {
            println!("   - 南京航空航天大学可能需要特定的cookies或JavaScript渲染");
            println!("   - 建议使用headless浏览器（如chromium）");
        } else if self.url.contains("hunu.edu.cn") {
            println!("   - 湖南大学服务器暂时不可用（503错误）");
            println!("   - 建议稍后重试或联系网站管理员");
        } else if self.url.contains("shufe.edu.cn") {
            println!("   - 上海财经大学可能响应较慢或有时限限制");
            println!("   - 建议增加超时时间或使用代理");
        }
    }
}

#[derive(Debug, Default)]
pub struct DiagnosisReport {
    pub name: String,
    pub url: String,
    pub dns_ok: bool,
    pub tcp_ok: bool,
    pub tcp_duration: Option<Duration>,
    pub http_no_headers: Option<(String, Duration)>,
    pub http_with_headers: Option<(String, Duration)>,
    pub ssl_ok: bool,
    pub content_size: Option<usize>,
}

/// 批量诊断多个网站
pub fn diagnose_failed_schools() -> Result<()> {
    let failed_schools = vec![
        ("南京航空航天大学", "http://www.nuaa.edu.cn/"),
        ("湖南大学", "http://www.hunu.edu.cn/"),
        ("上海财经大学", "http://www.shufe.edu.cn/"),
    ];

    println!("🚀 开始诊断失败的学校网站...\n");

    for (name, url) in failed_schools {
        let diagnoser = SiteDiagnoser::new(name.to_string(), url.to_string());
        if let Err(e) = diagnoser.diagnose() {
            eprintln!("❌ 诊断{}时发生错误: {}", name, e);
        }
    }

    println!("\n✅ 诊断完成！");

    Ok(())
}
