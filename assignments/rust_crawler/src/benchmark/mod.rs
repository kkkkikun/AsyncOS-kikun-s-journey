use crate::utils::{SchoolInfo, Config, PerformanceStats};
use crate::crawlers::{ProcessCrawler, ThreadCrawler, AsyncCrawler, StdThreadCrawler, StreamAsyncCrawler};
use anyhow::{Result, Context};

/// 性能对比结果
#[derive(Debug)]
pub struct ComparisonResult {
    pub process_stats: PerformanceStats,
    pub thread_stats: PerformanceStats,
    pub async_stats: PerformanceStats,
}

/// 性能对比框架
pub struct BenchmarkRunner {
    config: Config,
}

impl BenchmarkRunner {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// 运行完整的性能对比测试
    pub async fn run_comparison(&self, schools: Vec<SchoolInfo>) -> Result<ComparisonResult> {
        println!("\n{}", "=".repeat(60));
        println!("🏁 开始性能对比测试");
        println!("📊 测试数据量: {} 个学校", schools.len());
        println!("⚙️  并发数量: {}", self.config.concurrency);
        println!("{}", "=".repeat(60));

        // 1. 运行进程爬虫
        println!("\n📦 [1/3] 运行基于进程的爬虫...");
        let process_crawler = ProcessCrawler::new(self.config.clone());
        let process_stats = process_crawler.run(schools.clone()).await?;

        // 清理数据目录，准备下一次测试
        self.clean_data_directory();

        // 2. 运行线程爬虫
        println!("\n🧵 [2/3] 运行基于线程的爬虫...");
        let thread_stats = tokio::task::spawn_blocking({
            let config = self.config.clone();
            let schools = schools.clone();
            move || {
                let thread_crawler = ThreadCrawler::new(config);
                thread_crawler.run(schools)
            }
        })
        .await
        .context("线程爬虫执行失败")??;

        // 清理数据目录
        self.clean_data_directory();

        // 3. 运行协程爬虫
        println!("\n⚡ [3/3] 运行基于协程的爬虫...");
        let async_crawler = AsyncCrawler::new(self.config.clone());
        let async_stats = async_crawler.run(schools).await?;

        Ok(ComparisonResult {
            process_stats,
            thread_stats,
            async_stats,
        })
    }

    /// 运行单个爬虫的基准测试
    pub async fn run_single(
        &self,
        crawler_type: CrawlerType,
        schools: Vec<SchoolInfo>,
    ) -> Result<PerformanceStats> {
        match crawler_type {
            CrawlerType::Process => {
                let crawler = ProcessCrawler::new(self.config.clone());
                crawler.run(schools).await
            }
            CrawlerType::Thread => {
                let crawler = ThreadCrawler::new(self.config.clone());
                crawler.run(schools)
            }
            CrawlerType::Async => {
                let crawler = AsyncCrawler::new(self.config.clone());
                crawler.run(schools).await
            }
            CrawlerType::StdThread => {
                let crawler = StdThreadCrawler::new(self.config.clone());
                crawler.run(schools)
            }
            CrawlerType::StreamAsync => {
                let crawler = StreamAsyncCrawler::new(self.config.clone());
                crawler.run(schools).await
            }
        }
    }

    fn clean_data_directory(&self) {
        // 只删除测试结果文件，完整保留缓存目录
        let output_path = std::path::Path::new(&self.config.output_dir);
        let cache_dir = output_path.join("cache");

        // 删除.txt文件（爬取结果），但完整保留cache目录及其内容
        if let Ok(entries) = std::fs::read_dir(output_path) {
            for entry in entries.flatten() {
                let path = entry.path();

                // 跳过缓存目录本身及其内部所有文件
                if path == cache_dir || path.starts_with(&cache_dir) {
                    continue;
                }

                // 只删除.txt文件
                if path.extension().and_then(|s| s.to_str()) == Some("txt") {
                    if let Err(e) = std::fs::remove_file(&path) {
                        eprintln!("⚠️  删除文件失败: {:?}, 错误: {}", path, e);
                    }
                }
            }
        }

        // 确保缓存目录存在且完整
        if !cache_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&cache_dir) {
                eprintln!("⚠️  创建缓存目录失败: {}", e);
            }
        }

        // 确保保护文件存在
        let protect_file = cache_dir.join(".cache_protect");
        if !protect_file.exists() {
            if let Err(e) = std::fs::write(&protect_file, "MOCK_CACHE_DIRECTORY - DO_NOT_DELETE") {
                eprintln!("⚠️  创建缓存保护文件失败: {}", e);
            }
        }
    }
}

/// 爬虫类型
#[derive(Debug, Clone, Copy)]
pub enum CrawlerType {
    Process,
    Thread,
    Async,
    StdThread,
    StreamAsync,
}

impl CrawlerType {
    pub fn name(&self) -> &str {
        match self {
            CrawlerType::Process => "进程爬虫",
            CrawlerType::Thread => "线程爬虫",
            CrawlerType::Async => "协程爬虫",
            CrawlerType::StdThread => "标准线程爬虫",
            CrawlerType::StreamAsync => "流式协程爬虫",
        }
    }
}
