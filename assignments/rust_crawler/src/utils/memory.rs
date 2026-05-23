/// Memory tracking utilities for Linux
/// Reads VmRSS from /proc/self/status to track actual memory usage

use std::fs;
use anyhow::{Result, Context};

/// Memory statistics for a process
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// Peak Resident Set Size (VmHWM - High Water Mark) in KB
    /// This is the maximum physical RAM the process has used since start
    pub peak_rss_kb: usize,
    /// Current Resident Set Size in KB
    pub current_rss_kb: usize,
    /// Virtual memory size in KB
    pub vm_size_kb: usize,
}

impl MemoryStats {
    /// Read memory stats from /proc/self/status
    pub fn read_from_proc() -> Result<Self> {
        let content = fs::read_to_string("/proc/self/status")
            .context("Failed to read /proc/self/status")?;

        let mut peak_rss_kb = 0usize;
        let mut current_rss_kb = 0usize;
        let mut vm_size_kb = 0usize;

        for line in content.lines() {
            if line.starts_with("VmHWM:") {
                peak_rss_kb = parse_proc_value(line)?;
            } else if line.starts_with("VmRSS:") {
                current_rss_kb = parse_proc_value(line)?;
            } else if line.starts_with("VmSize:") {
                vm_size_kb = parse_proc_value(line)?;
            }
        }

        Ok(MemoryStats {
            peak_rss_kb,
            current_rss_kb,
            vm_size_kb,
        })
    }

    /// Get current RSS in MB
    pub fn current_rss_mb(&self) -> f64 {
        self.current_rss_kb as f64 / 1024.0
    }

    /// Get peak RSS in MB
    pub fn peak_rss_mb(&self) -> f64 {
        self.peak_rss_kb as f64 / 1024.0
    }

    /// Get VM size in MB
    pub fn vm_size_mb(&self) -> f64 {
        self.vm_size_kb as f64 / 1024.0
    }
}

/// Parse a value from /proc/self/status line
/// Format: "FieldName:  value kB"
fn parse_proc_value(line: &str) -> Result<usize> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].parse::<usize>()
            .context(format!("Failed to parse value from line: {}", line))
    } else {
        anyhow::bail!("Invalid proc status line format: {}", line)
    }
}

/// Memory tracker for benchmark runs
/// Tracks baseline and peak to compute accurate delta measurements
pub struct MemoryTracker {
    /// Memory stats when tracker was initialized (baseline)
    baseline_stats: MemoryStats,
    /// Peak memory stats observed during tracking
    peak_stats: MemoryStats,
}

impl MemoryTracker {
    /// Start tracking memory with a known baseline
    /// The baseline represents memory usage BEFORE the workload starts
    pub fn start_with_baseline(baseline: MemoryStats) -> Self {
        MemoryTracker {
            baseline_stats: baseline.clone(),
            peak_stats: baseline,
        }
    }

    /// Start tracking memory from current state as baseline
    pub fn start() -> Result<Self> {
        let baseline = MemoryStats::read_from_proc()?;
        Ok(Self::start_with_baseline(baseline))
    }

    /// Update peak stats (call periodically during benchmark)
    /// Returns the current memory delta in MB if called
    pub fn update(&mut self) -> Result<f64> {
        let current = MemoryStats::read_from_proc()?;
        if current.current_rss_kb > self.peak_stats.current_rss_kb {
            self.peak_stats = current;
        }
        Ok(self.delta_mb())
    }

    /// Get the peak memory usage during tracking (absolute value in MB)
    pub fn peak_mb(&self) -> f64 {
        self.peak_stats.peak_rss_mb()
    }

    /// Get the baseline memory usage (absolute value in MB)
    pub fn baseline_mb(&self) -> f64 {
        self.baseline_stats.current_rss_mb()
    }

    /// Get the memory delta (peak - baseline) in MB
    /// This is the net memory attributable to the workload
    pub fn delta_mb(&self) -> f64 {
        (self.peak_stats.current_rss_kb - self.baseline_stats.current_rss_kb) as f64 / 1024.0
    }

    /// Get the peak delta (VmHWM - baseline) in MB
    /// Uses VmHWM (High Water Mark) which is the maximum resident set size
    /// the process has ever reached since it started
    pub fn peak_delta_mb(&self) -> f64 {
        (self.peak_stats.peak_rss_kb - self.baseline_stats.current_rss_kb) as f64 / 1024.0
    }
}

/// Read current process RSS only (lightweight version)
pub fn read_current_rss_kb() -> Result<usize> {
    let content = fs::read_to_string("/proc/self/status")
        .context("Failed to read /proc/self/status")?;

    for line in content.lines() {
        if line.starts_with("VmRSS:") {
            return parse_proc_value(line);
        }
    }

    anyhow::bail!("VmRSS not found in /proc/self/status")
}

/// Read current process RSS in MB
pub fn read_current_rss_mb() -> Result<f64> {
    Ok(read_current_rss_kb()? as f64 / 1024.0)
}

/// Read memory stats for a specific process by PID
pub fn read_process_memory_stats(pid: u32) -> Result<MemoryStats> {
    let path = format!("/proc/{}/status", pid);
    let content = fs::read_to_string(&path)
        .context(format!("Failed to read /proc/{}/status (process may have exited)", pid))?;

    let mut peak_rss_kb = 0usize;
    let mut current_rss_kb = 0usize;
    let mut vm_size_kb = 0usize;

    for line in content.lines() {
        if line.starts_with("VmHWM:") {
            peak_rss_kb = parse_proc_value(line)?;
        } else if line.starts_with("VmRSS:") {
            current_rss_kb = parse_proc_value(line)?;
        } else if line.starts_with("VmSize:") {
            vm_size_kb = parse_proc_value(line)?;
        }
    }

    Ok(MemoryStats {
        peak_rss_kb,
        current_rss_kb,
        vm_size_kb,
    })
}

/// Read current RSS for a specific process by PID (returns 0 if process doesn't exist)
pub fn read_process_rss_kb(pid: u32) -> Option<usize> {
    match read_process_memory_stats(pid) {
        Ok(stats) => Some(stats.current_rss_kb),
        Err(_) => None, // Process may have exited
    }
}

/// Aggregate memory usage from multiple child processes
/// Returns sum of current RSS in KB for all PIDs that are still alive
pub fn aggregate_child_process_rss(pids: &[u32]) -> usize {
    pids.iter()
        .filter_map(|&pid| read_process_rss_kb(pid))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_memory_stats() {
        let stats = MemoryStats::read_from_proc().unwrap();
        println!("Current RSS: {:.2} MB", stats.current_rss_mb());
        println!("Peak RSS: {:.2} MB", stats.peak_rss_mb());
        println!("VM Size: {:.2} MB", stats.vm_size_mb());

        // Sanity checks
        assert!(stats.current_rss_kb > 0);
        assert!(stats.current_rss_mb() > 0.0);
    }

    #[test]
    fn test_memory_tracker() {
        let mut tracker = MemoryTracker::start().unwrap();
        tracker.update().unwrap();

        let delta = tracker.delta_mb();
        println!("Memory delta: {:.2} MB", delta);
    }
}
