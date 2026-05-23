use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

/// 获取33个中国高校列表（硬编码）
fn get_schools() -> Vec<(String, String)> {
    vec![
        ("北京大学".to_string(), "https://www.pku.edu.cn/".to_string()),
        ("清华大学".to_string(), "https://www.tsinghua.edu.cn/".to_string()),
        ("中国人民大学".to_string(), "https://www.ruc.edu.cn/".to_string()),
        ("北京师范大学".to_string(), "https://www.bnu.edu.cn/".to_string()),
        ("北京航空航天大学".to_string(), "https://www.buaa.edu.cn/".to_string()),
        ("北京理工大学".to_string(), "https://www.bit.edu.cn/".to_string()),
        ("中国农业大学".to_string(), "https://www.cau.edu.cn/".to_string()),
        ("中央民族大学".to_string(), "https://www.muc.edu.cn/".to_string()),
        ("中国科学院大学".to_string(), "https://www.ucas.ac.cn/".to_string()),
        ("复旦大学".to_string(), "https://www.fudan.edu.cn/".to_string()),
        ("上海交通大学".to_string(), "https://www.sjtu.edu.cn/".to_string()),
        ("同济大学".to_string(), "https://www.tongji.edu.cn/".to_string()),
        ("华东师范大学".to_string(), "https://www.ecnu.edu.cn/".to_string()),
        ("上海财经大学".to_string(), "https://www.shufe.edu.cn/".to_string()),
        ("南开大学".to_string(), "https://www.nankai.edu.cn/".to_string()),
        ("天津大学".to_string(), "https://www.tju.edu.cn/".to_string()),
        ("重庆大学".to_string(), "https://www.cqu.edu.cn/".to_string()),
        ("南京大学".to_string(), "https://www.nju.edu.cn/".to_string()),
        ("东南大学".to_string(), "https://www.seu.edu.cn/".to_string()),
        ("南京航空航天大学".to_string(), "https://www.nuaa.edu.cn/".to_string()),
        ("浙江大学".to_string(), "https://www.zju.edu.cn/".to_string()),
        ("中国科学技术大学".to_string(), "https://www.ustc.edu.cn/".to_string()),
        ("武汉大学".to_string(), "https://www.whu.edu.cn/".to_string()),
        ("华中科技大学".to_string(), "https://www.hust.edu.cn/".to_string()),
        ("中南大学".to_string(), "https://www.csu.edu.cn/".to_string()),
        ("湖南大学".to_string(), "https://www.hnu.edu.cn/".to_string()),
        ("中山大学".to_string(), "https://www.sysu.edu.cn/".to_string()),
        ("华南理工大学".to_string(), "https://www.scut.edu.cn/".to_string()),
        ("四川大学".to_string(), "https://www.scu.edu.cn/".to_string()),
        ("电子科技大学".to_string(), "https://www.uestc.edu.cn/".to_string()),
        ("西安交通大学".to_string(), "https://www.xjtu.edu.cn/".to_string()),
        ("西北工业大学".to_string(), "https://www.nwpu.edu.cn/".to_string()),
        ("兰州大学".to_string(), "https://www.lzu.edu.cn/".to_string()),
    ]
}

fn main() -> Result<(), Box<dyn Error>> {
    // 1. 定义并创建保存数据的目录 `./data`
    let data_dir = "./data";
    if !Path::new(data_dir).exists() {
        fs::create_dir_all(data_dir)?;
        println!("✅ 成功创建目录：{}", data_dir);
    }

    // 2. 获取33个学校列表（硬编码）
    let schools = get_schools();
    println!("📊 准备爬取 {} 个学校\n", schools.len());

    // 3. 构建 HTTP 客户端
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    println!("🚀 开始进行顺序爬取任务...\n");

    let mut success_count = 0;
    let mut fail_count = 0;
    let total = schools.len();

    // 4. 遍历每个学校
    for (index, (name, url)) in schools.iter().enumerate() {
        println!("🔍 [{}/{}] 正在爬取: 【{}】", index + 1, total, name);
        println!("   URL: {}", url);

        match client.get(url).send() {
            Ok(response) => {
                let status = response.status();

                if status.is_success() {
                    match response.text() {
                        Ok(html_content) => {
                            println!("   ✅ 状态码: {} | 响应大小: {} bytes", status, html_content.len());

                            // 保存文件
                            let file_path = format!("{}/{}.txt", data_dir, name);
                            match File::create(&file_path) {
                                Ok(mut f) => {
                                    if let Err(e) = f.write_all(html_content.as_bytes()) {
                                        eprintln!("   ❌ 写入文件失败: {}", e);
                                        fail_count += 1;
                                    } else {
                                        println!("   💾 成功保存: {}", file_path);
                                        success_count += 1;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("   ❌ 创建文件失败: {}", e);
                                    fail_count += 1;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("   ❌ 读取内容失败: {}", e);
                            fail_count += 1;
                        }
                    }
                } else {
                    eprintln!("   ❌ HTTP错误: {}", status);
                    fail_count += 1;
                }
            }
            Err(e) => {
                eprintln!("   ❌ 请求失败: {}", e);
                fail_count += 1;
            }
        }

        println!("   {}", "─".repeat(60));

        // 添加延迟
        thread::sleep(Duration::from_millis(1000));
    }

    // 5. 显示最终统计
    println!("\n{}", "=".repeat(60));
    println!("📊 爬取任务完成！");
    println!("   总数: {} 个", total);
    println!("   ✅ 成功: {} 个", success_count);
    println!("   ❌ 失败: {} 个", fail_count);
    println!("   📈 成功率: {:.1}%", (success_count as f64 / total as f64 * 100.0));
    println!("{}", "=".repeat(60));

    // 6. 列出成功保存的文件（前10个和最后5个）
    println!("\n📁 已保存的文件（前10个）:");
    let saved_files = std::fs::read_dir(data_dir)?;
    let mut files: Vec<String> = saved_files
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".txt"))
        .collect::<Vec<_>>();

    files.sort();

    for (i, filename) in files.iter().take(10).enumerate() {
        println!("   {}. {}", i + 1, filename);
    }

    if files.len() > 10 {
        println!("   ...");
        for filename in files.iter().skip(files.len() - 5) {
            println!("   {}. {}", files.iter().position(|f| f == filename).unwrap() + 1, filename);
        }
    }

    println!("\n💾 文件保存目录: {}", std::fs::canonicalize(data_dir)?.display());

    Ok(())
}
