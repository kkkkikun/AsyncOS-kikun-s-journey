use anyhow::{Result, Context};
use std::fs;
use std::path::PathBuf;
use crate::utils::SchoolInfo;

/// 缓存管理器
pub struct CacheManager {
    cache_dir: PathBuf,
}

impl CacheManager {
    /// 创建新的缓存管理器
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    /// 获取默认缓存目录
    pub fn default_cache_dir() -> PathBuf {
        PathBuf::from("./data/cache")
    }

    /// 初始化缓存目录（确保存在，不会被删除）
    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.cache_dir)
            .context("创建缓存目录失败")?;

        // 创建保护标记文件，防止意外删除
        let protect_file = self.cache_dir.join(".cache_protect");
        if !protect_file.exists() {
            fs::write(&protect_file, "MOCK_CACHE_DIRECTORY - DO_NOT_DELETE")?;
        }

        println!("✅ 缓存目录已创建并保护: {:?}", self.cache_dir);
        Ok(())
    }

    /// 检查缓存目录是否有效（存在且包含保护文件）
    pub fn is_valid(&self) -> bool {
        let protect_file = self.cache_dir.join(".cache_protect");
        self.cache_dir.exists() && protect_file.exists()
    }

    /// 下载并缓存所有学校的内容
    pub async fn download_all(&self, schools: &[SchoolInfo]) -> Result<()> {
        println!("🌐 开始下载 {} 个学校的首页内容...", schools.len());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .context("创建HTTP客户端失败")?;

        let mut success_count = 0;
        let mut fail_count = 0;

        for (i, school) in schools.iter().enumerate() {
            println!("📥 [{}/{}] 下载: {}...", i + 1, schools.len(), school.name);

            match self.download_single(&client, school).await {
                Ok(_) => {
                    success_count += 1;
                    println!("   ✅ 成功");
                }
                Err(e) => {
                    fail_count += 1;
                    eprintln!("   ❌ 失败: {}", e);
                }
            }
        }

        println!("\n📊 下载完成:");
        println!("   成功: {} 个", success_count);
        println!("   失败: {} 个", fail_count);
        println!("   成功率: {:.1}%", (success_count as f64 / schools.len() as f64) * 100.0);

        // 写入保护文件
        self.init()?;

        Ok(())
    }

    /// 下载单个学校的内容并缓存
    async fn download_single(&self, client: &reqwest::Client, school: &SchoolInfo) -> Result<()> {
        let response = client.get(&school.url)
            .send()
            .await
            .context("发送HTTP请求失败")?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP状态码错误: {}", response.status());
        }

        let html_content = response.text()
            .await
            .context("读取响应内容失败")?;

        let cache_path = self.get_cache_path(school);
        fs::write(&cache_path, html_content)
            .context("写入缓存文件失败")?;

        Ok(())
    }

    /// 获取缓存文件路径
    pub fn get_cache_path(&self, school: &SchoolInfo) -> PathBuf {
        self.cache_dir.join(format!("{}.html", school.name))
    }

    /// 检查缓存是否存在
    pub fn cache_exists(&self, school: &SchoolInfo) -> bool {
        self.get_cache_path(school).exists()
    }

    /// 读取缓存内容
    pub fn read_cache(&self, school: &SchoolInfo) -> Result<String> {
        let cache_path = self.get_cache_path(school);
        fs::read_to_string(&cache_path)
            .with_context(|| format!("读取缓存文件失败: {:?}", cache_path))
    }

    /// 获取缓存统计信息
    pub fn get_cache_stats(&self, schools: &[SchoolInfo]) -> CacheStats {
        let cached_count = schools.iter()
            .filter(|s| self.cache_exists(s))
            .count();

        CacheStats {
            total_schools: schools.len(),
            cached_count,
            cache_dir: self.cache_dir.clone(),
        }
    }

    /// 清理过期的缓存（可选功能）
    pub fn cleanup(&self) -> Result<()> {
        // 只清理损坏的缓存文件，保留有效的
        println!("🧹 检查缓存完整性...");

        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            let mut removed_count = 0;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("html") {
                    // 检查文件是否为空或损坏
                    if let Ok(metadata) = fs::metadata(&path) {
                        if metadata.len() < 50 { // 小于50字节肯定是损坏的文件
                            fs::remove_file(&path)?;
                            removed_count += 1;
                        }
                    }
                }
            }
            if removed_count > 0 {
                println!("   清理了 {} 个损坏的缓存文件", removed_count);
            } else {
                println!("   ✅ 所有缓存文件都完整");
            }
        }

        Ok(())
    }
}

/// 缓存统计信息
pub struct CacheStats {
    pub total_schools: usize,
    pub cached_count: usize,
    pub cache_dir: PathBuf,
}

impl CacheStats {
    pub fn cache_rate(&self) -> f64 {
        if self.total_schools == 0 {
            0.0
        } else {
            (self.cached_count as f64 / self.total_schools as f64) * 100.0
        }
    }

    pub fn is_complete(&self) -> bool {
        self.cached_count == self.total_schools
    }
}
