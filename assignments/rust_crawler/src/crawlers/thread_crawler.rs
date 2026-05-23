use crate::utils::{SchoolInfo, PerformanceMonitor, PerformanceStats, Config, MemoryTracker, MemoryStats};
use crate::utils::{html_parser, file_handler, create_blocking_client_builder};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use anyhow::{Result, Context};
use rayon::prelude::*;
use reqwest::blocking::Client;

/// 基于线程的爬虫
///
/// 工作原理：
/// 1. 使用线程池处理多个URL
/// 2. 线程之间共享内存空间
/// 3. 使用互斥锁保护共享状态
///
/// 性能特征：
/// - 中等内存开销：线程共享进程内存，但每个线程有独立栈
/// - 线程创建开销中等：创建线程比进程快，但仍涉及系统调用
/// - 共享内存：需要同步机制，可能导致竞争
pub struct ThreadCrawler {
    config: Config,
}

impl ThreadCrawler {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// 运行线程爬虫
    pub fn run(&self, schools: Vec<SchoolInfo>) -> Result<PerformanceStats> {
        let monitor = Arc::new(Mutex::new(PerformanceMonitor::new()));

        // 使用改进的HTTP客户端配置
        let client = Arc::new(
            create_blocking_client_builder(self.config.request_timeout())
                .build()
                .context("创建HTTP客户端失败")?
        );

        println!("\n🧵 [线程爬虫] 使用线程池，并发数: {}...", self.config.concurrency);
        if self.config.is_pure_io_mode() {
            println!("   纯I/O模式: 已启用 (跳过HTML解析)");
        }

        // Capture baseline after initialization
        let baseline = MemoryStats::read_from_proc()
            .context("Failed to read baseline memory")?;
        let mut memory_tracker = MemoryTracker::start_with_baseline(baseline);

        if !self.config.is_silent_mode() {
            println!("   内存基线: {:.2} MB", memory_tracker.baseline_mb());
        }

        let start_time = std::time::Instant::now();

        // 使用rayon线程池
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.concurrency)
            .build()
            .context("创建线程池失败")?;

        // 并行处理所有学校
        pool.install(|| {
            schools.par_iter().enumerate().for_each(|(i, school)| {
                let start = std::time::Instant::now();

                // 只在非静默模式下打印开始日志
                if !self.config.is_silent_mode() {
                    println!("🧵 线程 [{}/{}] 开始爬取: {}", i + 1, schools.len(), school.name);
                }

                let result = self.crawl_single_thread(
                    &client,
                    school,
                    i
                );

                let duration = start.elapsed();
                let (success, bytes) = match result {
                    Ok(bytes) => (true, bytes),
                    Err(e) => {
                        // 只在非静默模式下打印错误
                        if !self.config.is_silent_mode() {
                            eprintln!("❌ 爬取失败 [{}]: {}", school.name, e);
                        }
                        (false, 0)
                    }
                };

                monitor.lock().unwrap().record_task(
                    school.name.clone(),
                    duration,
                    success,
                    bytes
                );

                // 只在非静默模式下打印完成日志
                if !self.config.is_silent_mode() {
                    println!("✅ 线程 [{}/{}] 完成: {} (耗时: {:.2}s)",
                        i + 1, schools.len(), school.name, duration.as_secs_f64()
                    );
                }
            });
        });

        let total_time = start_time.elapsed();
        let mut stats = monitor.lock().unwrap().calculate_stats();
        stats.total_duration = total_time;

        // Final memory update and capture DELTA
        memory_tracker.update()?;
        let net_memory_mb = memory_tracker.peak_delta_mb();  // Use VmHWM for accurate peak measurement

        if !self.config.is_silent_mode() {
            println!("   内存增量: {:.2} MB (基线: {:.2} MB → 峰值: {:.2} MB)",
                net_memory_mb,
                memory_tracker.baseline_mb(),
                memory_tracker.baseline_mb() + net_memory_mb
            );
        }

        stats.peak_memory_mb = Some(net_memory_mb);

        Ok(stats)
    }

    /// 单个线程爬取函数
    fn crawl_single_thread(
        &self,
        client: &Arc<Client>,
        school: &SchoolInfo,
        index: usize,
    ) -> Result<usize> {
        // 人工延迟（用于放大性能差异）
        if let Some(delay) = self.config.artificial_delay() {
            // 添加随机性避免所有线程同时等待
            let random_delay = delay + Duration::from_millis((index % 10) as u64 * 10);
            std::thread::sleep(random_delay);
        }

        // 获取目标URL（在Mock模式下转换为本地URL）
        let target_url = self.config.maybe_convert_to_mock_url(&school.name, &school.url);

        // 发送HTTP请求
        let response = client
            .get(&target_url)
            .send()
            .context(format!("HTTP请求失败: {}", target_url))?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP状态码错误: {}", response.status());
        }

        let html = response
            .text()
            .context("读取响应内容失败")?;

        // Pure I/O mode: Skip HTML parsing, just count bytes
        if self.config.is_pure_io_mode() {
            return Ok(html.len());
        }

        // 提取文本内容
        let text = html_parser::extract_text_from_html(&html)?;

        // 保存到文件
        file_handler::save_text_to_file(&text, &school.name, &self.config.output_dir)?;

        Ok(text.len())
    }
}

/// 使用std::thread的替代实现（不使用rayon）
pub struct StdThreadCrawler {
    config: Config,
}

impl StdThreadCrawler {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn run(&self, schools: Vec<SchoolInfo>) -> Result<PerformanceStats> {
        let monitor = Arc::new(Mutex::new(PerformanceMonitor::new()));

        println!("\n🧵 [标准线程爬虫] 使用std::thread，并发数: {}...", self.config.concurrency);
        if self.config.is_pure_io_mode() {
            println!("   纯I/O模式: 已启用 (跳过HTML解析)");
        }

        // Capture baseline after initialization
        let baseline = MemoryStats::read_from_proc()
            .context("Failed to read baseline memory")?;
        let mut memory_tracker = MemoryTracker::start_with_baseline(baseline);

        if !self.config.is_silent_mode() {
            println!("   内存基线: {:.2} MB", memory_tracker.baseline_mb());
        }

        let start_time = std::time::Instant::now();

        // 创建任务通道
        let (task_sender, task_receiver) = std::sync::mpsc::channel();
        let (result_sender, result_receiver) = std::sync::mpsc::channel();

        // 发送所有任务
        for school in schools {
            task_sender.send(school).unwrap();
        }
        drop(task_sender); // 关闭发送端

        // 使用Arc包装Receiver以便在多个线程间共享
        let task_receiver = Arc::new(std::sync::Mutex::new(task_receiver));

        // 启动工作线程
        let mut handles = vec![];
        for worker_id in 0..self.config.concurrency {
            let task_receiver = task_receiver.clone();
            let result_sender = result_sender.clone();
            let config = self.config.clone();

            let handle = std::thread::spawn(move || {
                let client = create_blocking_client_builder(config.request_timeout())
                    .build()
                    .unwrap();

                loop {
                    // 尝试获取任务
                    let school = {
                        let receiver = task_receiver.lock().unwrap();
                        receiver.try_recv()
                    };

                    match school {
                        Ok(school) => {
                            let start = std::time::Instant::now();

                            let result = Self::crawl_task(&client, &school, worker_id, &config);

                            let duration = start.elapsed();
                            let (success, bytes) = match result {
                                Ok(bytes) => (true, bytes),
                                Err(_) => (false, 0),
                            };

                            result_sender.send((school, duration, success, bytes)).unwrap();
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // 没有任务，短暂休眠后重试
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            // 通道已关闭，退出线程
                            break;
                        }
                    }
                }
            });

            handles.push(handle);
        }
        drop(result_sender);

        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }

        // 收集结果
        for (school, duration, success, bytes) in result_receiver {
            monitor.lock().unwrap().record_task(
                school.name,
                duration,
                success,
                bytes
            );
        }

        let total_time = start_time.elapsed();
        let mut stats = monitor.lock().unwrap().calculate_stats();
        stats.total_duration = total_time;

        // Final memory update and capture DELTA
        memory_tracker.update()?;
        let net_memory_mb = memory_tracker.peak_delta_mb();  // Use VmHWM for accurate peak measurement

        if !self.config.is_silent_mode() {
            println!("   内存增量: {:.2} MB", net_memory_mb);
        }

        stats.peak_memory_mb = Some(net_memory_mb);

        Ok(stats)
    }

    fn crawl_task(
        client: &Client,
        school: &SchoolInfo,
        worker_id: usize,
        config: &Config,
    ) -> Result<usize> {
        if !config.is_silent_mode() {
            println!("🧵 工作线程 {} 爬取: {}", worker_id, school.name);
        }

        // 人工延迟
        if let Some(delay) = config.artificial_delay() {
            std::thread::sleep(delay);
        }

        let target_url = config.maybe_convert_to_mock_url(&school.name, &school.url);

        let response = client
            .get(&target_url)
            .send()
            .context(format!("请求失败: {}", target_url))?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP错误: {}", response.status());
        }

        let html = response.text()?;

        // Pure I/O mode: Skip HTML parsing, just count bytes
        if config.is_pure_io_mode() {
            return Ok(html.len());
        }

        let text = html_parser::extract_text_from_html(&html)?;
        file_handler::save_text_to_file(&text, &school.name, &config.output_dir)?;

        Ok(text.len())
    }
}
