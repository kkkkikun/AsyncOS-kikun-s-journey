use crate::utils::{SchoolInfo, PerformanceMonitor, PerformanceStats, Config};
use std::process::{Command, Stdio};
use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::{Result, Context};

/// Child process lifespan event for memory tracking
#[derive(Debug, Clone)]
struct ProcessMemoryEvent {
    start_time: Instant,
    end_time: Instant,
    peak_rss_kb: usize,
}

#[derive(Debug, Clone, Copy)]
enum EventType {
    Start,
    End,
}

#[derive(Debug, Clone)]
struct TimelineEvent {
    timestamp: Duration,
    event_type: EventType,
    peak_rss_kb: usize,
}

/// 基于进程的爬虫
///
/// 工作原理：
/// 1. 为每个URL创建一个独立的子进程
/// 2. 进程之间通过stdin/stdout进行通信
/// 3. 主进程等待所有子进程完成
///
/// 性能特征：
/// - 高内存开销：每个进程都有独立的内存空间
/// - 进程创建开销大：创建和销毁进程需要系统调用
/// - 内存隔离：进程之间互不影响，更加稳定
pub struct ProcessCrawler {
    config: Config,
}

impl ProcessCrawler {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// 运行进程爬虫
    pub async fn run(&self, schools: Vec<SchoolInfo>) -> Result<PerformanceStats> {
        let mut monitor = PerformanceMonitor::new();

        println!("\n🔧 [进程爬虫] 启动 {} 个子进程...", self.config.concurrency);
        if self.config.is_pure_io_mode() {
            println!("   纯I/O模式: 已启用 (跳过HTML解析)");
        }

        let start_time = std::time::Instant::now();

        // 创建任务队列
        let mut tasks: Vec<_> = schools.into_iter().enumerate().collect();

        let silent_mode = self.config.is_silent_mode();
        let pure_io_mode = self.config.is_pure_io_mode();

        // 使用通道来收集完成的任务
        let (result_sender, result_receiver) = std::sync::mpsc::channel();

        // Track process lifespan events for accurate concurrent peak memory calculation
        let process_events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let benchmark_start = Instant::now();

        // 控制并发数
        let mut running = 0;
        let mut completed = 0;
        let total = tasks.len();

        while !tasks.is_empty() || running > 0 {
            // 启动新进程直到达到并发限制
            while running < self.config.concurrency && !tasks.is_empty() {
                if let Some((_index, school)) = tasks.pop() {
                    let spawn_time = Instant::now();
                    let result_sender = result_sender.clone();
                    let config = self.config.clone();
                    let process_events = process_events.clone();

                    running += 1;

                    std::thread::spawn(move || {
                        let result = Self::run_single_process_with_memory(
                            &school, &config, silent_mode, pure_io_mode, spawn_time, benchmark_start, process_events
                        );

                        let duration = spawn_time.elapsed();
                        let _ = result_sender.send((school, duration, result));
                    });
                }
            }

            // 收集完成的结果
            if let Ok((school, duration, result)) = result_receiver.recv() {
                running -= 1;
                completed += 1;

                let (success, bytes) = match result {
                    Ok((success, bytes)) => (success, bytes),
                    Err(e) => {
                        if !silent_mode {
                            eprintln!("❌ 进程执行失败 [{}]: {}", school.name, e);
                        }
                        (false, 0)
                    }
                };

                if !silent_mode {
                    println!("✅ 进程 [{}/{}] 完成: {} (耗时: {:.2}s)",
                        completed, total, school.name, duration.as_secs_f64());
                }

                monitor.record_task(school.name, duration, success, bytes);
            }
        }

        let total_time = start_time.elapsed();
        let mut stats = monitor.calculate_stats();
        stats.total_duration = total_time;

        // Calculate true peak concurrent memory using sweep-line algorithm
        let process_events = process_events.lock().unwrap();
        let peak_concurrent_memory_mb = if !process_events.is_empty() {
            // Build timeline events
            let mut timeline_events: Vec<TimelineEvent> = Vec::new();

            for event in process_events.iter() {
                timeline_events.push(TimelineEvent {
                    timestamp: event.start_time.duration_since(benchmark_start),
                    event_type: EventType::Start,
                    peak_rss_kb: event.peak_rss_kb,
                });
                timeline_events.push(TimelineEvent {
                    timestamp: event.end_time.duration_since(benchmark_start),
                    event_type: EventType::End,
                    peak_rss_kb: event.peak_rss_kb,
                });
            }

            // Sort events chronologically
            // IMPORTANT: If timestamps are equal, Start events must precede End events
            // to ensure accurate sweep-line peak calculation
            timeline_events.sort_by(|a, b| {
                match a.timestamp.cmp(&b.timestamp) {
                    std::cmp::Ordering::Equal => {
                        // Stable ordering: Start before End when timestamps match
                        match (a.event_type, b.event_type) {
                            (EventType::Start, EventType::End) => std::cmp::Ordering::Less,
                            (EventType::End, EventType::Start) => std::cmp::Ordering::Greater,
                            _ => std::cmp::Ordering::Equal,
                        }
                    }
                    other => other,
                }
            });

            // Sweep through timeline to find maximum concurrent memory
            let mut current_concurrent_memory_kb = 0usize;
            let mut peak_concurrent_memory_kb = 0usize;

            for event in timeline_events {
                match event.event_type {
                    EventType::Start => {
                        current_concurrent_memory_kb += event.peak_rss_kb;
                        if current_concurrent_memory_kb > peak_concurrent_memory_kb {
                            peak_concurrent_memory_kb = current_concurrent_memory_kb;
                        }
                    }
                    EventType::End => {
                        current_concurrent_memory_kb -= event.peak_rss_kb;
                    }
                }
            }

            peak_concurrent_memory_kb as f64 / 1024.0
        } else {
            0.0
        };

        if !silent_mode {
            println!("   子进程并发内存峰值: {:.2} MB (基于 {} 个worker进程的时间线重建)",
                peak_concurrent_memory_mb, process_events.len());
        }

        stats.peak_memory_mb = Some(peak_concurrent_memory_mb);

        Ok(stats)
    }

    /// 运行单个进程任务并解析其自报告的内存使用（在独立线程中执行）
    fn run_single_process_with_memory(
        school: &SchoolInfo,
        config: &Config,
        _silent_mode: bool,
        _pure_io_mode: bool,
        spawn_time: Instant,
        benchmark_start: Instant,
        process_events: Arc<std::sync::Mutex<Vec<ProcessMemoryEvent>>>,
    ) -> Result<(bool, usize)> {
        use std::process::{Command, Stdio};
        use std::io::Read;

        // 获取目标URL（在Mock模式下转换为本地URL）
        let target_url = config.maybe_convert_to_mock_url(&school.name, &school.url);

        // 创建子进程，使用worker子命令
        let mut child = Command::new(std::env::current_exe()?)
            .arg("worker")  // 使用子命令而不是参数
            .arg("--url")
            .arg(&target_url)
            .arg("--name")
            .arg(&school.name)
            .arg("--output-dir")
            .arg(&config.output_dir)
            .arg("--timeout")
            .arg(config.request_timeout_ms.to_string())
            .arg("--pure-io")  // Pass pure-io mode flag
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("启动子进程失败")?;

        // 等待进程完成
        let status = child.wait()
            .context("等待子进程失败")?;

        // Record end time (process has exited)
        let end_time = Instant::now();

        // 读取进程输出（包含内存报告）
        let output = if let Some(mut stdout) = child.stdout.take() {
            let mut output_str = String::new();
            stdout.read_to_string(&mut output_str)
                .context("读取子进程stdout失败")?;
            output_str
        } else {
            String::new()
        };

        // 读取错误输出（用于调试）
        if let Some(mut stderr) = child.stderr.take() {
            let mut error_str = String::new();
            stderr.read_to_string(&mut error_str)
                .context("读取子进程stderr失败")?;
            if !error_str.is_empty() && !status.success() && !_silent_mode {
                eprintln!("⚠️  [{}] 进程错误: {}", school.name, error_str);
            }
        }

        // 解析内存报告
        let mut reported_memory_kb: Option<usize> = None;
        for line in output.lines() {
            if line.contains("__PEAK_RESIDENT_MEMORY_KB:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() == 2 {
                    if let Ok(kb) = parts[1].parse::<usize>() {
                        reported_memory_kb = Some(kb);
                    }
                }
            }
        }

        // Store process lifespan event for timeline reconstruction
        if let Some(peak_rss_kb) = reported_memory_kb {
            let event = ProcessMemoryEvent {
                start_time: spawn_time,
                end_time,
                peak_rss_kb,
            };
            process_events.lock().unwrap().push(event);
        }

        // 解析输出（处理业务逻辑结果）
        let (success, bytes) = if !output.trim().is_empty() {
            // Find the JSON line (not the memory report line)
            let json_lines: Vec<&str> = output.lines()
                .filter(|line| !line.contains("__PEAK_RESIDENT_MEMORY_KB:"))
                .collect();

            if !json_lines.is_empty() {
                let json_output = json_lines.join("\n");
                match serde_json::from_str::<serde_json::Value>(&json_output) {
                    Ok(result) => {
                        let success = result["success"].as_bool().unwrap_or(status.success());
                        let bytes = result["bytes"].as_u64().unwrap_or(0) as usize;
                        (success, bytes)
                    }
                    Err(e) => {
                        if !_silent_mode {
                            eprintln!("⚠️  [{}] 解析输出失败: {}, 输出: {}", school.name, e, json_output);
                        }
                        (status.success(), 0)
                    }
                }
            } else {
                (status.success(), 0)
            }
        } else {
            (status.success(), 0)
        };

        Ok((success, bytes))
    }
}

/// 作为worker进程执行的函数（由子进程调用）
pub fn run_worker_process(
    url: String,
    name: String,
    output_dir: String,
    timeout_ms: u64,
    pure_io: bool,
) -> Result<()> {
    use crate::utils::{html_parser, file_handler, create_blocking_client_builder, MemoryStats};
    use std::time::Duration;
    use std::io::Write;

    let start = std::time::Instant::now();

    // 创建HTTP客户端（使用改进的配置）
    let client = create_blocking_client_builder(Duration::from_millis(timeout_ms))
        .build()
        .context("创建HTTP客户端失败")?;

    // 发送请求
    let response = client
        .get(&url)
        .send()
        .context(format!("请求失败: {}", url))?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("HTTP错误: {}", status);
    }

    let html = response.text().context("读取响应内容失败")?;

    let duration = start.elapsed();

    // Pure I/O mode: Skip HTML parsing, just count bytes
    if pure_io {
        let output = serde_json::json!({
            "success": true,
            "name": name,
            "duration_ms": duration.as_millis(),
            "bytes": html.len()
        });

        println!("{}", output);
        std::io::stdout().flush().unwrap();

        // Self-report peak memory before exiting
        let memory_stats = MemoryStats::read_from_proc()
            .context("Failed to read process memory")?;
        println!("__PEAK_RESIDENT_MEMORY_KB:{}", memory_stats.peak_rss_kb);
        std::io::stdout().flush().unwrap();

        return Ok(());
    }

    // 提取文本
    let text = html_parser::extract_text_from_html(&html)
        .context("提取HTML文本失败")?;

    // 保存文件
    let file_path = file_handler::save_text_to_file(&text, &name, &output_dir)
        .context("保存文件失败")?;

    // 输出结果到stdout（供父进程读取）
    // 使用println!并手动刷新确保输出被发送
    let output = serde_json::json!({
        "success": true,
        "name": name,
        "file": file_path,
        "duration_ms": duration.as_millis(),
        "bytes": text.len()
    });

    println!("{}", output);
    std::io::stdout().flush().unwrap();

    // Self-report peak memory before exiting (always do this last)
    let memory_stats = MemoryStats::read_from_proc()
        .context("Failed to read process memory")?;
    println!("__PEAK_RESIDENT_MEMORY_KB:{}", memory_stats.peak_rss_kb);
    std::io::stdout().flush().unwrap();

    Ok(())
}
