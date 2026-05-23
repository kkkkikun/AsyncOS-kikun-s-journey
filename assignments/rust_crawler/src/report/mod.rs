use crate::benchmark::ComparisonResult;
use crate::utils::PerformanceStats;
use std::io::Write;

/// 性能对比报告生成器
pub struct ReportGenerator {
    output_path: String,
}

impl ReportGenerator {
    pub fn new(output_path: String) -> Self {
        Self { output_path }
    }

    /// 生成并保存完整的性能对比报告
    pub fn generate_report(&self, result: &ComparisonResult) -> std::io::Result<()> {
        let mut file = std::fs::File::create(&self.output_path)?;

        // 写入报告头部
        writeln!(file, "{}", "=".repeat(80))?;
        writeln!(file, "🔍 性能对比测试报告")?;
        writeln!(file, "📅 生成时间: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
        writeln!(file, "{}", "=".repeat(80))?;
        writeln!(file)?;

        // 写入性能对比表
        self.write_comparison_table(&mut file, result)?;

        // 写入详细分析
        self.write_detailed_analysis(&mut file, result)?;

        // 写入建议
        self.write_recommendations(&mut file, result)?;

        println!("\n✅ 报告已保存到: {}", self.output_path);
        Ok(())
    }

    /// 写入性能对比表
    fn write_comparison_table(&self, file: &mut std::fs::File, result: &ComparisonResult) -> std::io::Result<()> {
        writeln!(file, "📊 性能指标对比表")?;
        writeln!(file, "{}", "─".repeat(80))?;
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}", "指标", "进程爬虫", "线程爬虫", "协程爬虫")?;
        writeln!(file, "{}", "─".repeat(80))?;

        // 总任务数
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "总任务数",
            result.process_stats.total_tasks,
            result.thread_stats.total_tasks,
            result.async_stats.total_tasks
        )?;

        // 成功率
        let process_success_rate = success_rate(&result.process_stats);
        let thread_success_rate = success_rate(&result.thread_stats);
        let async_success_rate = success_rate(&result.async_stats);

        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "成功率(%)",
            format!("{:.2}", process_success_rate),
            format!("{:.2}", thread_success_rate),
            format!("{:.2}", async_success_rate)
        )?;

        // 总耗时
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "总耗时(秒)",
            format!("{:.2}", result.process_stats.total_duration.as_secs_f64()),
            format!("{:.2}", result.thread_stats.total_duration.as_secs_f64()),
            format!("{:.2}", result.async_stats.total_duration.as_secs_f64())
        )?;

        // 吞吐率
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "吞吐率(任务/秒)",
            format!("{:.2}", result.process_stats.throughput()),
            format!("{:.2}", result.thread_stats.throughput()),
            format!("{:.2}", result.async_stats.throughput())
        )?;

        // 平均延迟
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "平均延迟(毫秒)",
            format!("{:.2}", result.process_stats.avg_latency().as_millis()),
            format!("{:.2}", result.thread_stats.avg_latency().as_millis()),
            format!("{:.2}", result.async_stats.avg_latency().as_millis())
        )?;

        // P50延迟
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "P50延迟(毫秒)",
            format!("{:.2}", result.process_stats.p50_latency().as_millis()),
            format!("{:.2}", result.thread_stats.p50_latency().as_millis()),
            format!("{:.2}", result.async_stats.p50_latency().as_millis())
        )?;

        // P95延迟
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "P95延迟(毫秒)",
            format!("{:.2}", result.process_stats.p95_latency().as_millis()),
            format!("{:.2}", result.thread_stats.p95_latency().as_millis()),
            format!("{:.2}", result.async_stats.p95_latency().as_millis())
        )?;

        // P99延迟
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "P99延迟(毫秒)",
            format!("{:.2}", result.process_stats.p99_latency().as_millis()),
            format!("{:.2}", result.thread_stats.p99_latency().as_millis()),
            format!("{:.2}", result.async_stats.p99_latency().as_millis())
        )?;

        // 延迟标准差
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "延迟标准差(毫秒)",
            format!("{:.2}", result.process_stats.latency_std_dev() * 1000.0),
            format!("{:.2}", result.thread_stats.latency_std_dev() * 1000.0),
            format!("{:.2}", result.async_stats.latency_std_dev() * 1000.0)
        )?;

        // 下载数据量
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "总下载量(字节)",
            result.process_stats.total_bytes,
            result.thread_stats.total_bytes,
            result.async_stats.total_bytes
        )?;

        // 峰值内存
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "峰值内存(MB)",
            format!("{:.2}", result.process_stats.peak_memory_mb.unwrap_or(0.0)),
            format!("{:.2}", result.thread_stats.peak_memory_mb.unwrap_or(0.0)),
            format!("{:.2}", result.async_stats.peak_memory_mb.unwrap_or(0.0))
        )?;

        // 系统限制错误
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "系统限制错误",
            result.process_stats.system_limit_errors,
            result.thread_stats.system_limit_errors,
            result.async_stats.system_limit_errors
        )?;

        // 网络错误
        writeln!(file, "{:<20} {:<20} {:<20} {:<20}",
            "网络错误",
            result.process_stats.network_errors,
            result.thread_stats.network_errors,
            result.async_stats.network_errors
        )?;

        writeln!(file, "{}", "─".repeat(80))?;
        writeln!(file)?;
        Ok(())
    }

    /// 写入详细分析
    fn write_detailed_analysis(&self, file: &mut std::fs::File, result: &ComparisonResult) -> std::io::Result<()> {
        writeln!(file, "📈 详细分析")?;
        writeln!(file, "{}", "─".repeat(80))?;

        // 吞吐率分析
        let (best_throughput, throughput_winner) = [
            ("进程爬虫", result.process_stats.throughput()),
            ("线程爬虫", result.thread_stats.throughput()),
            ("协程爬虫", result.async_stats.throughput()),
        ]
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .unwrap();

        writeln!(file, "🏆 吞吐率冠军: {} ({:.2} 任务/秒)", throughput_winner, best_throughput)?;
        writeln!(file)?;

        // 延迟分析
        writeln!(file, "⏱️  延迟分析:")?;
        writeln!(file, "   - 平均延迟最低: {} ({:.2} 毫秒)",
            if result.process_stats.avg_latency() < result.thread_stats.avg_latency()
                && result.process_stats.avg_latency() < result.async_stats.avg_latency() {
                "进程爬虫"
            } else if result.thread_stats.avg_latency() < result.async_stats.avg_latency() {
                "线程爬虫"
            } else {
                "协程爬虫"
            },
            result.process_stats.avg_latency()
                .min(result.thread_stats.avg_latency())
                .min(result.async_stats.avg_latency())
                .as_millis()
        )?;

        writeln!(file, "   - P95延迟最低: {} ({:.2} 毫秒)",
            if result.process_stats.p95_latency() < result.thread_stats.p95_latency()
                && result.process_stats.p95_latency() < result.async_stats.p95_latency() {
                "进程爬虫"
            } else if result.thread_stats.p95_latency() < result.async_stats.p95_latency() {
                "线程爬虫"
            } else {
                "协程爬虫"
            },
            result.process_stats.p95_latency()
                .min(result.thread_stats.p95_latency())
                .min(result.async_stats.p95_latency())
                .as_millis()
        )?;

        writeln!(file)?;

        // 延迟稳定性分析
        writeln!(file, "📊 延迟稳定性 (标准差越小越稳定):")?;
        writeln!(file, "   - 进程爬虫: {:.2} 毫秒", result.process_stats.latency_std_dev() * 1000.0)?;
        writeln!(file, "   - 线程爬虫: {:.2} 毫秒", result.thread_stats.latency_std_dev() * 1000.0)?;
        writeln!(file, "   - 协程爬虫: {:.2} 毫秒", result.async_stats.latency_std_dev() * 1000.0)?;
        writeln!(file)?;

        Ok(())
    }

    /// 写入建议
    fn write_recommendations(&self, file: &mut std::fs::File, result: &ComparisonResult) -> std::io::Result<()> {
        writeln!(file, "💡 选择建议")?;
        writeln!(file, "{}", "─".repeat(80))?;

        writeln!(file, "🔹 进程爬虫 (Process-based):")?;
        writeln!(file, "   ✅ 优点: 内存隔离、稳定性高、故障隔离好")?;
        writeln!(file, "   ❌ 缺点: 启动开销大、内存占用高、IPC通信复杂")?;
        writeln!(file, "   🎯 适用场景: 任务独立性强、对稳定性要求高、任务数量少")?;
        writeln!(file)?;

        writeln!(file, "🔹 线程爬虫 (Thread-based):")?;
        writeln!(file, "   ✅ 优点: 共享内存、开发简单、适合CPU密集型任务")?;
        writeln!(file, "   ❌ 缺点: 线程创建开销、需要同步机制、栈空间占用")?;
        writeln!(file, "   🎯 适用场景: CPU密集型任务、需要共享状态、中等并发")?;
        writeln!(file)?;

        writeln!(file, "🔹 协程爬虫 (Async/Await):")?;
        writeln!(file, "   ✅ 优点: 内存占用极低、高效处理I/O、易于扩展")?;
        writeln!(file, "   ❌ 缺点: 异步编程复杂度、不适合CPU密集型任务")?;
        writeln!(file, "   🎯 适用场景: I/O密集型任务、高并发、大规模爬取")?;
        writeln!(file)?;

        writeln!(file, "{}", "=".repeat(80))?;
        writeln!(file, "📝 结论: 根据测试结果，建议根据具体场景选择合适的并发模型")?;
        writeln!(file, "{}", "=".repeat(80))?;

        Ok(())
    }

    /// 打印报告到控制台
    pub fn print_summary(&self, result: &ComparisonResult) {
        println!("\n{}", "=".repeat(80));
        println!("📊 性能对比测试完成 - 结果摘要");
        println!("{}", "=".repeat(80));

        println!("\n┌──────────────────┬────────────┬────────────┬────────────┐");
        println!("│ 指标             │ 进程爬虫   │ 线程爬虫   │ 协程爬虫   │");
        println!("├──────────────────┼────────────┼────────────┼────────────┤");

        println!("│ 总耗时(秒)       │ {:>10.2} │ {:>10.2} │ {:>10.2} │",
            result.process_stats.total_duration.as_secs_f64(),
            result.thread_stats.total_duration.as_secs_f64(),
            result.async_stats.total_duration.as_secs_f64()
        );

        println!("│ 吞吐率(任务/秒)  │ {:>10.2} │ {:>10.2} │ {:>10.2} │",
            result.process_stats.throughput(),
            result.thread_stats.throughput(),
            result.async_stats.throughput()
        );

        println!("│ 平均延迟(毫秒)   │ {:>10.2} │ {:>10.2} │ {:>10.2} │",
            result.process_stats.avg_latency().as_millis(),
            result.thread_stats.avg_latency().as_millis(),
            result.async_stats.avg_latency().as_millis()
        );

        println!("│ P95延迟(毫秒)    │ {:>10.2} │ {:>10.2} │ {:>10.2} │",
            result.process_stats.p95_latency().as_millis(),
            result.thread_stats.p95_latency().as_millis(),
            result.async_stats.p95_latency().as_millis()
        );

        println!("│ 成功率(%)        │ {:>10.2} │ {:>10.2} │ {:>10.2} │",
            success_rate(&result.process_stats),
            success_rate(&result.thread_stats),
            success_rate(&result.async_stats)
        );

        println!("│ 峰值内存(MB)     │ {:>10.2} │ {:>10.2} │ {:>10.2} │",
            result.process_stats.peak_memory_mb.unwrap_or(0.0),
            result.thread_stats.peak_memory_mb.unwrap_or(0.0),
            result.async_stats.peak_memory_mb.unwrap_or(0.0)
        );

        println!("└──────────────────┴────────────┴────────────┴────────────┘");

        // 确定冠军
        let throughput_winner = if result.process_stats.throughput() > result.thread_stats.throughput()
            && result.process_stats.throughput() > result.async_stats.throughput() {
            "进程爬虫"
        } else if result.thread_stats.throughput() > result.async_stats.throughput() {
            "线程爬虫"
        } else {
            "协程爬虫"
        };

        println!("\n🏆 吞吐率冠军: {}", throughput_winner);
        println!("{}", "=".repeat(80));
    }
}

fn success_rate(stats: &PerformanceStats) -> f64 {
    if stats.total_tasks > 0 {
        (stats.successful_tasks as f64 / stats.total_tasks as f64) * 100.0
    } else {
        0.0
    }
}
