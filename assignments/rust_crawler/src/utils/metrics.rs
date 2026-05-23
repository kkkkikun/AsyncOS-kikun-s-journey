use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};

/// 单个任务的性能指标
#[derive(Debug, Clone, Serialize)]
pub struct TaskMetrics {
    pub task_id: String,
    #[serde(skip)] // Instant不能序列化
    pub start_time: Instant,
    pub duration: Duration,
    pub success: bool,
    pub bytes_downloaded: usize,
}

/// 总体性能统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    pub total_tasks: usize,
    pub successful_tasks: usize,
    pub failed_tasks: usize,
    pub total_duration: Duration,
    pub total_bytes: usize,
    pub latency_samples: Vec<Duration>,
    pub peak_memory_mb: Option<f64>,

    // Enhanced error diagnostics
    pub system_limit_errors: usize,  // EMFILE, ENOMEM, etc.
    pub network_errors: usize,       // Connection errors, timeouts
    pub parse_errors: usize,         // HTML parsing errors
    pub io_errors: usize,            // File I/O errors
}

impl PerformanceStats {
    pub fn new() -> Self {
        Self {
            total_tasks: 0,
            successful_tasks: 0,
            failed_tasks: 0,
            total_duration: Duration::ZERO,
            total_bytes: 0,
            latency_samples: Vec::new(),
            peak_memory_mb: None,
            system_limit_errors: 0,
            network_errors: 0,
            parse_errors: 0,
            io_errors: 0,
        }
    }

    pub fn add_task(&mut self, metrics: &TaskMetrics) {
        self.total_tasks += 1;
        if metrics.success {
            self.successful_tasks += 1;
        } else {
            self.failed_tasks += 1;
        }
        self.total_duration += metrics.duration;
        self.total_bytes += metrics.bytes_downloaded;
        self.latency_samples.push(metrics.duration);
    }

    /// 计算吞吐率（任务/秒）
    pub fn throughput(&self) -> f64 {
        if self.total_duration.as_secs_f64() > 0.0 {
            self.successful_tasks as f64 / self.total_duration.as_secs_f64()
        } else {
            0.0
        }
    }

    /// 计算平均延迟
    pub fn avg_latency(&self) -> Duration {
        if self.latency_samples.is_empty() {
            return Duration::ZERO;
        }
        let sum: Duration = self.latency_samples.iter().sum();
        sum / self.latency_samples.len() as u32
    }

    /// 计算中位数延迟（P50）
    pub fn p50_latency(&self) -> Duration {
        self.percentile_latency(50)
    }

    /// 计算P95延迟
    pub fn p95_latency(&self) -> Duration {
        self.percentile_latency(95)
    }

    /// 计算P99延迟
    pub fn p99_latency(&self) -> Duration {
        self.percentile_latency(99)
    }

    fn percentile_latency(&self, percentile: u8) -> Duration {
        if self.latency_samples.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted = self.latency_samples.clone();
        sorted.sort();
        let index = (sorted.len() as f64 * percentile as f64 / 100.0) as usize;
        sorted.get(index.min(sorted.len() - 1)).copied().unwrap_or(Duration::ZERO)
    }

    /// 计算延迟的标准差
    pub fn latency_std_dev(&self) -> f64 {
        if self.latency_samples.len() < 2 {
            return 0.0;
        }
        let avg = self.avg_latency().as_secs_f64();
        let variance = self.latency_samples.iter()
            .map(|d| {
                let diff = d.as_secs_f64() - avg;
                diff * diff
            })
            .sum::<f64>() / (self.latency_samples.len() - 1) as f64;
        variance.sqrt()
    }
}

/// 性能测量器
pub struct PerformanceMonitor {
    start_time: Instant,
    task_metrics: Vec<TaskMetrics>,
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            task_metrics: Vec::new(),
        }
    }

    pub fn record_task(&mut self, task_id: String, duration: Duration, success: bool, bytes: usize) {
        self.task_metrics.push(TaskMetrics {
            task_id,
            start_time: self.start_time,
            duration,
            success,
            bytes_downloaded: bytes,
        });
    }

    pub fn calculate_stats(&self) -> PerformanceStats {
        let mut stats = PerformanceStats::new();
        for metrics in &self.task_metrics {
            stats.add_task(metrics);
        }
        stats
    }

    pub fn get_elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// 延迟直方图（用于统计分析）
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    bins: Vec<(Duration, usize)>, // (时间范围, 计数)
}

impl LatencyHistogram {
    pub fn new(samples: &[Duration], num_bins: usize) -> Self {
        if samples.is_empty() {
            return Self {
                bins: Vec::new(),
            };
        }

        let mut sorted = samples.to_vec();
        sorted.sort();

        let min = sorted.first().copied().unwrap_or(Duration::ZERO);
        let max = sorted.last().copied().unwrap_or(Duration::ZERO);
        let range = max - min;

        let bin_size = if range > Duration::ZERO && num_bins > 1 {
            range / num_bins as u32
        } else {
            Duration::from_millis(1)
        };

        let mut bins = vec![(Duration::ZERO, 0); num_bins];
        for sample in &sorted {
            let bin_index = if bin_size > Duration::ZERO {
                ((*sample - min).as_millis() / bin_size.as_millis()) as usize
            } else {
                0
            };
            let bin_index = bin_index.min(num_bins - 1);
            bins[bin_index].0 = min + bin_size * bin_index as u32;
            bins[bin_index].1 += 1;
        }

        Self { bins }
    }

    pub fn display(&self) {
        println!("\n📊 延迟分布直方图:");
        println!("┌─────────────────────┬──────────┐");
        println!("│ 延迟范围           │ 数量     │");
        println!("├─────────────────────┼──────────┤");
        for (range, count) in &self.bins {
            println!("│ {:>8}ms - {:>8}ms │ {:>8} │",
                range.as_millis(),
                (range.as_millis() + 100),
                count
            );
        }
        println!("└─────────────────────┴──────────┘");
    }
}
