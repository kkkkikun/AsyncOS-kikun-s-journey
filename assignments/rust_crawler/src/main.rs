mod utils;
mod crawlers;
mod benchmark;
mod report;
mod diagnose;

use anyhow::Result;
use clap::{Parser, Subcommand};
use utils::*;
use benchmark::{BenchmarkRunner, CrawlerType};
use report::ReportGenerator;
use diagnose::diagnose_failed_schools;

#[derive(Parser)]
#[command(name = "rust_crawler")]
#[command(about = "Rust并发爬虫性能对比工具", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 运行所有爬虫并进行性能对比
    Compare {
        /// 并发数量
        #[arg(short, long, default_value_t = 10)]
        concurrency: usize,

        /// 请求超时时间（毫秒）
        #[arg(long, default_value_t = 5000)]
        timeout: u64,

        /// 人工延迟（毫秒）用于放大性能差异
        #[arg(long, default_value_t = 100)]
        delay: u64,

        /// 禁用人工延迟
        #[arg(long, default_value_t = false)]
        no_delay: bool,

        /// 启用Mock模式（本地离线测试）
        #[arg(long, default_value_t = false)]
        mock: bool,

        /// Mock模式下的重复次数
        #[arg(long, default_value_t = 500)]
        repeat: usize,

        /// 启用纯I/O模式（跳过HTML解析）
        #[arg(long, default_value_t = false)]
        pure_io: bool,

        /// 输出目录
        #[arg(long, default_value = "./data")]
        output: String,

        /// 报告文件路径
        #[arg(long, default_value = "./comparison_report.txt")]
        report: String,
    },

    /// 运行单个爬虫
    Run {
        /// 爬虫类型
        #[arg(long, value_enum)]
        crawler: CrawlerTypeChoice,

        /// 并发数量
        #[arg(short, long, default_value_t = 10)]
        concurrency: usize,

        /// 请求超时时间（毫秒）
        #[arg(long, default_value_t = 5000)]
        timeout: u64,

        /// 启用Mock模式（本地离线测试）
        #[arg(long, default_value_t = false)]
        mock: bool,

        /// Mock模式下的重复次数
        #[arg(long, default_value_t = 500)]
        repeat: usize,

        /// 启用纯I/O模式（跳过HTML解析）
        #[arg(long, default_value_t = false)]
        pure_io: bool,
    },

    /// 下载并缓存所有学校的内容（用于Mock模式）
    Download {
        /// 缓存目录路径
        #[arg(long, default_value = "./data/cache")]
        cache_dir: String,
    },

    /// 工作进程（内部使用）
    Worker {
        /// URL to crawl
        #[arg(long)]
        url: String,

        /// School name
        #[arg(long)]
        name: String,

        /// Output directory
        #[arg(long, default_value = "./data")]
        output_dir: String,

        /// Timeout in milliseconds
        #[arg(long, default_value_t = 5000)]
        timeout: u64,

        /// Pure I/O mode (skip HTML parsing)
        #[arg(long, default_value_t = false)]
        pure_io: bool,
    },

    /// 诊断失败的学校网站
    Diagnose,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum CrawlerTypeChoice {
    Process,
    Thread,
    Async,
    StdThread,
    StreamAsync,
}

impl From<CrawlerTypeChoice> for CrawlerType {
    fn from(value: CrawlerTypeChoice) -> Self {
        match value {
            CrawlerTypeChoice::Process => CrawlerType::Process,
            CrawlerTypeChoice::Thread => CrawlerType::Thread,
            CrawlerTypeChoice::Async => CrawlerType::Async,
            CrawlerTypeChoice::StdThread => CrawlerType::StdThread,
            CrawlerTypeChoice::StreamAsync => CrawlerType::StreamAsync,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compare {
            concurrency,
            timeout,
            delay,
            no_delay,
            mock,
            repeat,
            pure_io,
            output,
            report,
        } => {
            // 创建运行时并运行异步函数
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_comparison(
                concurrency,
                timeout,
                delay,
                !no_delay,
                mock,
                repeat,
                pure_io,
                output,
                report,
            ))?
        }

        Commands::Run {
            crawler,
            concurrency,
            timeout,
            mock,
            repeat,
            pure_io,
        } => {
            // 创建运行时并运行异步函数
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_single_crawler(crawler.into(), concurrency, timeout, mock, repeat, pure_io))?
        }

        Commands::Download { cache_dir } => {
            // 创建运行时并运行异步函数
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_download(cache_dir))?
        }

        Commands::Worker {
            url,
            name,
            output_dir,
            timeout,
            pure_io,
        } => {
            // Worker命令在同步上下文中运行
            crawlers::process_crawler::run_worker_process(url, name, output_dir, timeout, pure_io)?;
        }

        Commands::Diagnose => {
            diagnose_failed_schools()?;
        }
    }

    Ok(())
}

async fn run_comparison(
    concurrency: usize,
    timeout_ms: u64,
    delay_ms: u64,
    enable_delay: bool,
    mock_mode: bool,
    mock_repeat: usize,
    pure_io_mode: bool,
    output_dir: String,
    report_path: String,
) -> Result<()> {
    println!("🚀 启动并发爬虫性能对比测试...");

    // 使用硬编码的学校数据
    let schools = get_schools();

    if schools.is_empty() {
        anyhow::bail!("没有找到有效的学校数据");
    }

    println!("📊 加载了 {} 个学校", schools.len());

    // Validate concurrency vs total requests
    let test_schools = if mock_mode {
        expand_school_list(&schools, mock_repeat)
    } else {
        schools.clone()
    };

    if concurrency > test_schools.len() {
        println!("⚠️  警告: 并发数 ({}) 大于总任务数 ({}), 将有 {} 个worker实体无法获得任务",
            concurrency, test_schools.len(), concurrency - test_schools.len());
        println!("💡 建议: 减少 --concurrency 或增加 --repeat 以确保充分利用所有worker");
    }

    // 如果启用Mock模式，启动Mock服务器
    let mock_base_url = if mock_mode {
        println!("\n🔧 启用Mock模式（本地离线测试）...");

        let cache_dir = CacheManager::default_cache_dir();
        let cache_manager = CacheManager::new(cache_dir.clone());

        // 检查缓存状态
        let stats = cache_manager.get_cache_stats(&schools);

        if !stats.is_complete() {
            println!("📁 缓存状态: {}/{} ({:.1}%) - 不完整",
                stats.cached_count, stats.total_schools, stats.cache_rate());
            println!("💡 提示: 运行 `cargo run -- download` 下载所有学校内容");
            println!("🔄 将使用现有缓存继续测试...");
        } else {
            println!("📁 缓存状态: {}/{} ({:.1}%) - ✅ 完整",
                stats.cached_count, stats.total_schools, stats.cache_rate());
        }

        // 暂时禁用缓存清理，确保缓存文件不被误删
        // let _ = cache_manager.cleanup();

        // 启动Mock服务器
        let mock_server = MockServerRunner::start(cache_dir).await?;
        Some(mock_server.base_url().to_string())
    } else {
        None
    };

    // 创建配置
    let mut config = Config::default()
        .with_concurrency(concurrency)
        .with_timeout(timeout_ms)
        .with_artificial_delay(delay_ms, enable_delay)
        .with_mock_mode(mock_mode)
        .with_mock_repeat(mock_repeat)
        .with_silent_mode(mock_mode)  // Mock模式自动启用静默模式
        .with_pure_io_mode(pure_io_mode);  // 纯I/O模式

    if let Some(ref url) = mock_base_url {
        config = config.with_mock_base_url(url.clone());
    }

    config.output_dir = output_dir.clone();

    // Print test configuration
    if mock_mode {
        println!("🔄 Mock模式: 学校列表已扩展 {} 倍，总请求数: {}", mock_repeat, test_schools.len());
    }

    // 运行对比测试
    let runner = BenchmarkRunner::new(config);
    let result = runner.run_comparison(test_schools).await?;

    // 生成报告
    let generator = ReportGenerator::new(report_path);
    generator.generate_report(&result)?;
    generator.print_summary(&result);

    Ok(())
}

async fn run_single_crawler(
    crawler_type: CrawlerType,
    concurrency: usize,
    timeout_ms: u64,
    mock_mode: bool,
    mock_repeat: usize,
    pure_io_mode: bool,
) -> Result<()> {
    println!("🚀 运行单个爬虫: {}", crawler_type.name());

    // 使用硬编码的学校数据
    let schools = get_schools();

    if schools.is_empty() {
        anyhow::bail!("没有找到有效的学校数据");
    }

    println!("📊 加载了 {} 个学校", schools.len());

    // 如果启用Mock模式，启动Mock服务器
    let mock_base_url = if mock_mode {
        println!("\n🔧 启用Mock模式（本地离线测试）...");

        let cache_dir = CacheManager::default_cache_dir();
        let cache_manager = CacheManager::new(cache_dir.clone());

        // 检查缓存状态
        let stats = cache_manager.get_cache_stats(&schools);

        if !stats.is_complete() {
            println!("📁 缓存状态: {}/{} ({:.1}%) - 不完整",
                stats.cached_count, stats.total_schools, stats.cache_rate());
            println!("💡 提示: 运行 `cargo run -- download` 下载所有学校内容");
            println!("🔄 将使用现有缓存继续测试...");
        } else {
            println!("📁 缓存状态: {}/{} ({:.1}%) - ✅ 完整",
                stats.cached_count, stats.total_schools, stats.cache_rate());
        }

        // 暂时禁用缓存清理，确保缓存文件不被误删
        // let _ = cache_manager.cleanup();

        // 启动Mock服务器
        let mock_server = MockServerRunner::start(cache_dir).await?;
        Some(mock_server.base_url().to_string())
    } else {
        None
    };

    // 创建配置
    let mut config = Config::default()
        .with_concurrency(concurrency)
        .with_timeout(timeout_ms)
        .with_artificial_delay(100, true)
        .with_mock_mode(mock_mode)
        .with_mock_repeat(mock_repeat)
        .with_silent_mode(mock_mode)  // Mock模式自动启用静默模式
        .with_pure_io_mode(pure_io_mode);  // 纯I/O模式

    if let Some(ref url) = mock_base_url {
        config = config.with_mock_base_url(url.clone());
    }

    // 在Mock模式下扩展学校列表
    let test_schools = if mock_mode {
        let expanded = expand_school_list(&schools, mock_repeat);
        println!("🔄 Mock模式: 学校列表已扩展 {} 倍，总请求数: {}", mock_repeat, expanded.len());
        expanded
    } else {
        schools
    };

    // 运行爬虫
    let runner = BenchmarkRunner::new(config);
    let stats = runner.run_single(crawler_type, test_schools).await?;

    // 打印结果
    println!("\n📊 性能统计:");
    println!("   总任务数: {}", stats.total_tasks);
    println!("   成功任务: {}", stats.successful_tasks);
    println!("   失败任务: {}", stats.failed_tasks);
    println!("   总耗时: {:.2} 秒", stats.total_duration.as_secs_f64());
    println!("   吞吐率: {:.2} 任务/秒", stats.throughput());
    println!("   平均延迟: {:.2} 毫秒", stats.avg_latency().as_millis());

    Ok(())
}

/// 下载并缓存所有学校的内容
async fn run_download(cache_dir: String) -> Result<()> {
    println!("🌐 开始下载学校内容到缓存...");

    let cache_manager = CacheManager::new(&cache_dir);
    cache_manager.init()?;

    let schools = get_schools();
    cache_manager.download_all(&schools).await?;

    // 显示缓存统计
    let stats = cache_manager.get_cache_stats(&schools);
    println!("\n📁 缓存统计:");
    println!("   缓存目录: {:?}", stats.cache_dir);
    println!("   总学校数: {}", stats.total_schools);
    println!("   已缓存: {}", stats.cached_count);
    println!("   缓存率: {:.1}%", stats.cache_rate());
    println!("   状态: {}", if stats.is_complete() { "✅ 完整" } else { "❌ 不完整" });

    Ok(())
}

/// 扩展学校列表用于Mock模式压力测试
fn expand_school_list(schools: &[SchoolInfo], repeat_count: usize) -> Vec<SchoolInfo> {
    let mut expanded = Vec::with_capacity(schools.len() * repeat_count);

    for _ in 0..repeat_count {
        for (i, school) in schools.iter().enumerate() {
            expanded.push(SchoolInfo {
                id: expanded.len() + 1,
                name: school.name.clone(),
                url: school.url.clone(),
            });
        }
    }

    expanded
}
