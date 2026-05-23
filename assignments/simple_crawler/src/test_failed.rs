use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

fn test_single_url(name: &str, url: &str, output_dir: &str) -> Result<(), Box<dyn Error>> {
    println!("\n🔍 测试: {} -> {}", name, url);

    // 创建HTTP客户端
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15)) // 增加到15秒
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    println!("   发送请求...");

    match client.get(url).send() {
        Ok(mut response) => {
            let status = response.status();
            println!("   响应状态码: {}", status);
            println!("   响应Headers: ");

            // 打印一些重要的headers
            if let Some(content_type) = response.headers().get("content-type") {
                println!("     Content-Type: {:?}", content_type);
            }
            if let Some(content_length) = response.headers().get("content-length") {
                println!("     Content-Length: {:?}", content_length);
            }

            if status.is_success() {
                match response.text() {
                    Ok(html_content) => {
                        println!("   ✅ HTML内容长度: {} bytes", html_content.len());

                        // 保存到文件
                        let file_path = format!("{}/{}.txt", output_dir, name);
                        if let Ok(mut f) = File::create(&file_path) {
                            if let Err(e) = f.write_all(html_content.as_bytes()) {
                                println!("   ❌ 写入文件失败: {}", e);
                                return Err(e.into());
                            } else {
                                println!("   ✅ 成功保存: {}", file_path);
                            }
                        }

                        // 显示前100个字符预览
                        let preview = &html_content[..html_content.chars().take(100).collect::<String>().len()];
                        println!("   内容预览: {}...", preview);

                        Ok(())
                    }
                    Err(e) => {
                        println!("   ❌ 读取响应内容失败: {}", e);
                        Err(e.into())
                    }
                }
            } else {
                println!("   ❌ 请求失败，状态码: {}", status);

                // 尝试读取错误信息
                if let Ok(error_text) = response.text() {
                    println!("   错误信息: {}", error_text.chars().take(200).collect::<String>());
                }

                Err(format!("HTTP错误: {}", status).into())
            }
        }
        Err(e) => {
            println!("   ❌ 网络请求失败: {}", e);
            Err(e.into())
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let output_dir = "./test_data";

    // 创建输出目录
    if !Path::new(output_dir).exists() {
        std::fs::create_dir_all(output_dir)?;
        println!("创建测试目录: {}", output_dir);
    }

    println!("🚀 开始测试问题网站...\n");

    // 测试3个有问题的网站
    let test_sites = vec![
        ("南京航空航天大学", "https://www.nuaa.edu.cn/"),
        ("湖南大学", "https://www.hunu.edu.cn/"),
        ("上海财经大学", "https://www.sufe.edu.cn/"),
    ];

    let mut success_count = 0;
    let mut fail_count = 0;

    for (name, url) in test_sites {
        match test_single_url(name, url, output_dir) {
            Ok(_) => {
                success_count += 1;
                println!("   ✅ {} 测试成功", name);
            }
            Err(_) => {
                fail_count += 1;
                println!("   ❌ {} 测试失败", name);
            }
        }

        thread::sleep(Duration::from_secs(1));
        println!("{}", "─".repeat(60));
    }

    println!("\n📊 测试总结:");
    println!("   成功: {} 个", success_count);
    println!("   失败: {} 个", fail_count);
    println!("   成功率: {:.1}%", (success_count as f64 / 3.0 * 100.0));

    Ok(())
}
