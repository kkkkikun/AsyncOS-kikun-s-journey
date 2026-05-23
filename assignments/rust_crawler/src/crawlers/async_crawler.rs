use crate::utils::{SchoolInfo, PerformanceMonitor, PerformanceStats, Config, MemoryTracker, MemoryStats};
use crate::utils::{html_parser, file_handler, create_async_client_builder, get_timeout_for_url, get_retry_delay_for_url};
use std::sync::Arc;
use std::time::Duration;
use anyhow::{Result, Context};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::{sleep, timeout};

/// 基于协程（async/await）的爬虫
///
/// 工作原理：
/// 1. 使用tokio异步运行时
/// 2. 每个URL创建一个异步任务
/// 3. 使用信号量控制并发数量
///
/// 性能特征：
/// - 低内存开销：协程栈很小（几KB），可以创建数千个
/// - 协程创建开销极小：创建异步任务几乎无开销
/// - 高效I/O：在等待网络响应时可以执行其他任务
pub struct AsyncCrawler {
    config: Config,
}

impl AsyncCrawler {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// 运行异步爬虫（使用无锁通道聚合结果，优化高并发性能）
    pub async fn run(&self, schools: Vec<SchoolInfo>) -> Result<PerformanceStats> {
        // 使用无锁通道替代 Mutex，避免 3000+ 并发下的锁竞争瓶颈
        let (result_sender, mut result_receiver) = mpsc::unbounded_channel();
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));

        // 使用改进的HTTP客户端配置
        let client = Arc::new(
            create_async_client_builder(self.config.request_timeout())
                .build()
                .context("创建HTTP客户端失败")?
        );

        println!("\n⚡ [协程爬虫] 使用tokio异步运行时，并发数: {}...", self.config.concurrency);
        if self.config.is_pure_io_mode() {
            println!("   纯I/O模式: 已启用 (跳过HTML解析)");
        }

        // IMPORTANT: Capture memory baseline AFTER all initialization is done
        // but BEFORE spawning tasks. This gives us the "net" memory attributable
        // to the crawler workload, excluding Mock server overhead.
        let baseline = MemoryStats::read_from_proc()
            .context("Failed to read baseline memory")?;
        let mut memory_tracker = MemoryTracker::start_with_baseline(baseline);

        if !self.config.is_silent_mode() {
            println!("   内存基线: {:.2} MB", memory_tracker.baseline_mb());
        }

        let start_time = std::time::Instant::now();

        // 创建所有异步任务
        let mut tasks = tokio::task::JoinSet::new();

        for (i, school) in schools.into_iter().enumerate() {
            let permit = semaphore.clone();
            let client = client.clone();
            let sender = result_sender.clone();
            let config = self.config.clone();

            tasks.spawn(async move {
                // 获取信号量许可（限制并发数）
                let _permit = permit.acquire().await.unwrap();

                let start = std::time::Instant::now();

                // 只在非静默模式下打印开始日志
                if !config.is_silent_mode() {
                    println!("⚡ 协程 [{}/{}] 开始爬取: {}", i + 1, 1, school.name);
                }

                let result = Self::crawl_single_async(
                    &client,
                    &school,
                    &config,
                    i
                ).await;

                let duration = start.elapsed();
                let (success, bytes) = match result {
                    Ok(bytes) => (true, bytes),
                    Err(e) => {
                        // 只在非静默模式下打印错误
                        if !config.is_silent_mode() {
                            eprintln!("❌ 爬取失败 [{}]: {}", school.name, e);
                        }
                        (false, 0)
                    }
                };

                // 通过无锁通道发送结果，避免 mutex 竞争
                // Clone name before sending since we still need it for printing
                let _ = sender.send((school.name.clone(), duration, success, bytes));

                // 只在非静默模式下打印完成日志
                if !config.is_silent_mode() {
                    println!("✅ 协程 [{}/{}] 完成: {} (耗时: {:.2}s)",
                        i + 1, 1, school.name, duration.as_secs_f64()
                    );
                }
            });
        }

        // Drop the original sender so the receiver closes when all tasks finish
        drop(result_sender);

        // 等待所有任务完成并收集结果
        let mut monitor = PerformanceMonitor::new();
        while let Some((name, duration, success, bytes)) = result_receiver.recv().await {
            monitor.record_task(name, duration, success, bytes);
        }

        let total_time = start_time.elapsed();
        let mut stats = monitor.calculate_stats();
        stats.total_duration = total_time;

        // Final memory update and capture DELTA (net memory attributable to this crawler)
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

    /// 单个协程爬取函数（带重试机制和网站特定配置）
    async fn crawl_single_async(
        client: &reqwest::Client,
        school: &SchoolInfo,
        config: &Config,
        index: usize,
    ) -> Result<usize> {
        // 人工延迟（使用异步sleep）
        if let Some(delay) = config.artificial_delay() {
            // 添加随机性
            let random_delay = delay + Duration::from_millis((index % 10) as u64 * 10);
            sleep(random_delay).await;
        }

        // 获取目标URL（在Mock模式下转换为本地URL）
        let target_url = config.maybe_convert_to_mock_url(&school.name, &school.url);

        // 获取网站特定的超时配置
        let site_timeout = get_timeout_for_url(&target_url, config.request_timeout());
        let site_retry_delay = get_retry_delay_for_url(&target_url, Duration::from_secs(1));

        // 发送HTTP请求（异步，带重试逻辑）
        let max_retries = 3;
        let mut html_result = None;

        for attempt in 0..max_retries {
            let response = timeout(
                site_timeout,  // 使用网站特定的超时时间
                client.get(&target_url).send()
            )
            .await;

            match response {
                Ok(Ok(resp)) => {
                    if resp.status().is_success() {
                        // 尝试读取响应
                        // 根据网站调整读取超时时间
                        let read_timeout = if target_url.contains("shufe.edu.cn") || target_url.contains("127.0.0.1") {
                            Duration::from_secs(10) // 上海财经大学：10秒读取超时，Mock模式也使用较长超时
                        } else if target_url.contains("zju.edu.cn") {
                            Duration::from_secs(8) // 浙江大学：8秒读取超时
                        } else {
                            Duration::from_secs(5) // 默认：5秒读取超时
                        };

                        let text_result = timeout(
                            read_timeout,
                            resp.text()
                        ).await;

                        match text_result {
                            Ok(Ok(html)) => {
                                html_result = Some(html);
                                break;
                            }
                            Ok(Err(e)) => {
                                if attempt < max_retries - 1 {
                                    if !config.is_silent_mode() {
                                        eprintln!("⚠️  [{}] 读取响应失败(尝试{}/{}): {}, 将重试...",
                                            school.name, attempt + 1, max_retries, e);
                                    }
                                    sleep(site_retry_delay).await;
                                    continue;
                                } else {
                                    return Err(e).context("读取响应内容失败");
                                }
                            }
                            Err(_) => {
                                if attempt < max_retries - 1 {
                                    if !config.is_silent_mode() {
                                        eprintln!("⚠️  [{}] 读取响应超时(尝试{}/{}), 将重试...",
                                            school.name, attempt + 1, max_retries);
                                    }
                                    sleep(site_retry_delay).await;
                                    continue;
                                } else {
                                    anyhow::bail!("读取响应超时");
                                }
                            }
                        }
                    } else if attempt < max_retries - 1 {
                        if !config.is_silent_mode() {
                            eprintln!("⚠️  [{}] HTTP状态码错误: {} (尝试{}/{}), 将重试...",
                                school.name, resp.status(), attempt + 1, max_retries);
                        }
                        sleep(site_retry_delay).await;
                        continue;
                    } else {
                        anyhow::bail!("HTTP状态码错误: {}", resp.status());
                    }
                }
                Ok(Err(e)) => {
                    if attempt < max_retries - 1 {
                        if !config.is_silent_mode() {
                            eprintln!("⚠️  [{}] HTTP请求失败(尝试{}/{}): {}, 将重试...",
                                school.name, attempt + 1, max_retries, e);
                        }
                        sleep(site_retry_delay).await;
                        continue;
                    } else {
                        return Err(e).context("HTTP请求失败");
                    }
                }
                Err(_) => {
                    if attempt < max_retries - 1 {
                        if !config.is_silent_mode() {
                            eprintln!("⚠️  [{}] 请求超时(尝试{}/{}), 将重试...",
                                school.name, attempt + 1, max_retries);
                        }
                        sleep(Duration::from_millis(1000)).await;
                        continue;
                    } else {
                        anyhow::bail!("请求超时");
                    }
                }
            }
        }

        let html = html_result.ok_or_else(|| anyhow::anyhow!("无法获取HTML内容"))?;

        // Pure I/O mode: Skip HTML parsing, just count bytes
        if config.is_pure_io_mode() {
            return Ok(html.len());
        }

        // 提取文本（在异步上下文中执行CPU密集型任务）
        let text = tokio::task::spawn_blocking(move || {
            html_parser::extract_text_from_html(&html)
        })
        .await
        .context("解析任务失败")?
        .context("提取HTML文本失败")?;

        // 保存文件（也是阻塞操作）
        let _file_path = tokio::task::spawn_blocking({
            let text = text.clone();
            let name = school.name.clone();
            let output_dir = config.output_dir.clone();
            move || {
                file_handler::save_text_to_file(&text, &name, &output_dir)
            }
        })
        .await
        .context("保存任务失败")?
        .context("保存文件失败")?;

        Ok(text.len())
    }
}

/// 使用Futures流的替代实现（更函数式）
pub struct StreamAsyncCrawler {
    config: Config,
}

impl StreamAsyncCrawler {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn run(&self, schools: Vec<SchoolInfo>) -> Result<PerformanceStats> {
        use futures::stream::{self, StreamExt};

        let monitor = Arc::new(tokio::sync::Mutex::new(PerformanceMonitor::new()));

        let client = Arc::new(
            create_async_client_builder(self.config.request_timeout())
                .build()
                .context("创建HTTP客户端失败")?
        );

        println!("\n🌊 [流式协程爬虫] 使用futures流，并发数: {}...", self.config.concurrency);
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

        // 使用futures stream处理
        stream::iter(schools)
            .map(|school| {
                let client = client.clone();
                let monitor = monitor.clone();
                let config = self.config.clone();

                async move {
                    let start = std::time::Instant::now();

                    let result = Self::crawl_task(&client, &school, &config).await;

                    let duration = start.elapsed();
                    let (success, bytes) = match result {
                        Ok(bytes) => (true, bytes),
                        Err(_) => (false, 0),
                    };

                    monitor.lock().await.record_task(
                        school.name.clone(),
                        duration,
                        success,
                        bytes
                    );

                    duration
                }
            })
            .buffer_unordered(self.config.concurrency)
            .collect::<Vec<_>>()
            .await;

        let total_time = start_time.elapsed();
        let mut stats = monitor.lock().await.calculate_stats();
        stats.total_duration = total_time;

        // Final memory update and capture DELTA
        memory_tracker.update()?;
        let net_memory_mb = memory_tracker.delta_mb();

        if !self.config.is_silent_mode() {
            println!("   内存增量: {:.2} MB", net_memory_mb);
        }

        stats.peak_memory_mb = Some(net_memory_mb);

        Ok(stats)
    }

    async fn crawl_task(
        client: &reqwest::Client,
        school: &SchoolInfo,
        config: &Config,
    ) -> Result<usize> {
        if let Some(delay) = config.artificial_delay() {
            sleep(delay).await;
        }

        let target_url = config.maybe_convert_to_mock_url(&school.name, &school.url);

        let response = client.get(&target_url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("HTTP错误: {}", response.status());
        }

        let html = response.text().await?;

        // Pure I/O mode: Skip HTML parsing, just count bytes
        if config.is_pure_io_mode() {
            return Ok(html.len());
        }

        let text = html_parser::extract_text_from_html(&html)?;

        tokio::task::spawn_blocking({
            let text = text.clone();
            let name = school.name.clone();
            let output_dir = config.output_dir.clone();
            move || {
                file_handler::save_text_to_file(&text, &name, &output_dir)
            }
        })
        .await??;

        Ok(text.len())
    }
}
