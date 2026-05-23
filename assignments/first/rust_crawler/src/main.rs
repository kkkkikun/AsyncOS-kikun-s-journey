use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

/// 🛠️ 核心函数：读取本地 CSV 文件，将其解析为“学校名”和“官网网址”的元组数组
fn load_urls_from_csv() -> io::Result<Vec<(String, String)>> {
    let mut url_list = Vec::new();
    
    // 1. 指定读取根目录下的 urls.csv 文件
    let path = Path::new("urls.csv");
    let file = File::open(path)?;
    
    // 2. 使用 BufReader 按行高效读取，避免一次性把大文件吞进内存
    let reader = io::BufReader::new(file);

    for line in reader.lines() {
        let line_text = line?;
        
        // 3. 石墨导出的 CSV 默认是用英文逗号隔开的，我们将其切分
        // 格式一般为：院校名称,官方网站
        let parts: Vec<&str> = line_text.split(',').collect();
        
        if parts.len() >= 2 {
            let school_name = parts[0].trim().to_string();
            let url = parts[1].trim().to_string();
            
            // 4. 【数据清洗】严格过滤：必须是 http 或 https 开头，且排除表头本身
            if url.starts_with("http") && school_name != "院校名称" && school_name != "官方网站" {
                url_list.push((school_name, url));
            }
        }
    }
    Ok(url_list)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 [数据源准备] 正在加载本地高校数据...");
    
    // 调用函数加载数据
    match load_urls_from_csv() {
        Ok(schools) => {
            println!("✅ 成功从本地加载了 {} 个高校官方网站！", schools.len());
            
            println!("\n📊 [数据源抽样展示]:");
            // 抽样打印前 5 个学校，看看对不对
            for (index, (school, url)) in schools.iter().take(5).enumerate() {
                println!("  [{}] 学校: {} -> 官网: {}", index + 1, school, url);
            }
            
            println!("\n=======================================================");
            println!("🎉 阻碍全部清空！数据源已成功在内存中复活。");
            println!("接下来，我们可以正式开始编写【多线程】和【协程】并发爬虫了！");
            println!("=======================================================");
            
            // 💡 这里的 schools 变量（类型是 Vec<(String, String)>）
            // 就是你接下来喂给多线程线程池，或者 Tokio 异步任务大军的“子弹仓库”了！
        }
        Err(e) => {
            println!("❌ 加载失败！请检查 urls.csv 是否放对位置。错误原因: {}", e);
        }
    }

    Ok(())
}