use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    println!("🧪 测试修复后的编码处理...\n");

    // 测试数据：3个之前失败的学校
    let test_cases = vec![
        ("南京航空航天大学", "https://www.nuaa.edu.cn/"),
        ("湖南大学", "https://www.hnu.edu.cn/"),
        ("上海财经大学", "https://www.sufe.edu.cn/"),
    ];

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;

    let data_dir = "./data";
    let mut success_count = 0;
    let mut fail_count = 0;

    for (name, url) in test_cases {
        println!("🔍 测试: {} -> {}", name, url);

        match client.get(url).send() {
            Ok(response) => {
                println!("   状态码: {}", response.status());

                if response.status().is_success() {
                    match response.text() {
                        Ok(html_content) => {
                            println!("   ✅ 响应大小: {} bytes", html_content.len());

                            let file_path = format!("{}/{}.txt", data_dir, name);
                            match File::create(&file_path) {
                                Ok(mut f) => {
                                    if let Err(e) = f.write_all(html_content.as_bytes()) {
                                        println!("   ❌ 写入文件失败: {}", e);
                                        fail_count += 1;
                                    } else {
                                        println!("   ✅ 成功保存: {}", file_path);
                                        success_count += 1;

                                        // 显示内容预览
                                        let preview = &html_content[..html_content.chars().take(50).collect::<String>().len()];
                                        println!("   内容预览: {}...", preview);
                                    }
                                }
                                Err(e) => {
                                    println!("   ❌ 创建文件失败: {}", e);
                                    fail_count += 1;
                                }
                            }
                        }
                        Err(e) => {
                            println!("   ❌ 读取响应失败: {}", e);
                            fail_count += 1;
                        }
                    }
                } else {
                    println!("   ❌ HTTP错误: {}", response.status());
                    fail_count += 1;
                }
            }
            Err(e) => {
                println!("   ❌ 请求失败: {}", e);
                fail_count += 1;
            }
        }

        println!("   {}", "─".repeat(50));
        std::thread::sleep(Duration::from_millis(1000));
    }

    println!("\n📊 测试结果:");
    println!("   成功: {} 个", success_count);
    println!("   失败: {} 个", fail_count);
    println!("   成功率: {:.1}%", (success_count as f64 / 3.0 * 100.0));

    Ok(())
}
