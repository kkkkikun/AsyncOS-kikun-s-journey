use std::time::Duration;

/// 全局配置参数
#[derive(Clone)]
pub struct Config {
    /// 并发数量（进程/线程/协程）
    pub concurrency: usize,

    /// 请求超时时间（毫秒）
    pub request_timeout_ms: u64,

    /// 人工延迟用于放大性能差异（毫秒）
    pub artificial_delay_ms: u64,

    /// 是否启用人工延迟（用于测试对比）
    pub enable_artificial_delay: bool,

    /// 数据输出目录
    pub output_dir: String,

    /// CSV文件路径
    pub csv_path: String,

    /// 是否启用Mock模式（本地离线测试）
    pub mock_mode: bool,

    /// Mock模式下的重复次数
    pub mock_repeat: usize,

    /// Mock服务器基础URL
    pub mock_base_url: Option<String>,

    /// 静默模式（Mock模式自动启用）
    pub silent_mode: bool,

    /// 纯I/O模式（跳过HTML解析，只测量网络开销）
    pub pure_io_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            concurrency: 10,
            request_timeout_ms: 10000, // 增加到10秒，提高成功率
            artificial_delay_ms: 100, // 100ms的人工延迟，用于放大差异
            enable_artificial_delay: true, // 默认启用人工延迟
            output_dir: "./data".to_string(),
            csv_path: "./urls.CSV".to_string(),
            mock_mode: false,
            mock_repeat: 500,
            mock_base_url: None,
            silent_mode: false,
            pure_io_mode: false,
        }
    }
}

impl Config {
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.request_timeout_ms = timeout_ms;
        self
    }

    pub fn with_artificial_delay(mut self, delay_ms: u64, enabled: bool) -> Self {
        self.artificial_delay_ms = delay_ms;
        self.enable_artificial_delay = enabled;
        self
    }

    pub fn with_mock_mode(mut self, enabled: bool) -> Self {
        self.mock_mode = enabled;
        self
    }

    pub fn with_mock_repeat(mut self, repeat: usize) -> Self {
        self.mock_repeat = repeat;
        self
    }

    pub fn with_mock_base_url(mut self, url: String) -> Self {
        self.mock_base_url = Some(url);
        self
    }

    pub fn with_silent_mode(mut self, enabled: bool) -> Self {
        self.silent_mode = enabled;
        self
    }

    pub fn is_silent_mode(&self) -> bool {
        self.silent_mode
    }

    pub fn with_pure_io_mode(mut self, enabled: bool) -> Self {
        self.pure_io_mode = enabled;
        self
    }

    pub fn is_pure_io_mode(&self) -> bool {
        self.pure_io_mode
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms)
    }

    pub fn artificial_delay(&self) -> Option<Duration> {
        if self.enable_artificial_delay {
            Some(Duration::from_millis(self.artificial_delay_ms))
        } else {
            None
        }
    }

    /// 检查是否为Mock模式
    pub fn is_mock_mode(&self) -> bool {
        self.mock_mode
    }

    /// 获取Mock重复次数
    pub fn mock_repeat_count(&self) -> usize {
        if self.mock_mode {
            self.mock_repeat
        } else {
            1
        }
    }

    /// 将URL转换为Mock URL（如果启用Mock模式）
    pub fn maybe_convert_to_mock_url(&self, school_name: &str, original_url: &str) -> String {
        if let Some(ref base_url) = self.mock_base_url {
            // 将原始URL转换为Mock URL
            format!("{}/{}", base_url, school_name)
        } else {
            original_url.to_string()
        }
    }
}
