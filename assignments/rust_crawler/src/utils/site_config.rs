use crate::utils::{SchoolInfo, Config};
use std::time::Duration;

/// 获取网站特定的超时配置
pub fn get_timeout_for_url(url: &str, default_timeout: Duration) -> Duration {
    // 上海财经大学响应较慢，需要更长超时时间
    if url.contains("shufe.edu.cn") {
        Duration::from_secs(20) // 20秒超时
    }
    // 其他已知慢速网站可以在这里添加
    else if url.contains("zju.edu.cn") {
        Duration::from_secs(15) // 浙江大学：15秒
    }
    else if url.contains("bit.edu.cn") {
        Duration::from_secs(15) // 北京理工大学：15秒
    }
    else {
        default_timeout
    }
}

/// 获取网站特定的重试延迟
pub fn get_retry_delay_for_url(url: &str, default_delay: Duration) -> Duration {
    if url.contains("shufe.edu.cn") {
        Duration::from_millis(2000) // 2秒延迟
    } else {
        default_delay
    }
}

/// 检查URL是否需要特殊处理
pub fn needs_special_handling(url: &str) -> bool {
    url.contains("shufe.edu.cn") ||
    url.contains("nuaa.edu.cn") ||
    url.contains("hunu.edu.cn") ||
    url.contains("zju.edu.cn")
}

/// 获取网站描述信息
pub fn get_website_info(url: &str) -> Option<&'static str> {
    if url.contains("pku.edu.cn") {
        Some("北京大学")
    } else if url.contains("tsinghua.edu.cn") {
        Some("清华大学")
    } else if url.contains("shufe.edu.cn") {
        Some("上海财经大学 - 响应较慢")
    } else if url.contains("nuaa.edu.cn") {
        Some("南京航空航天大学 - 需要JavaScript渲染")
    } else if url.contains("hunu.edu.cn") {
        Some("湖南大学 - 服务器不稳定")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_configuration() {
        let default = Duration::from_secs(10);

        // 测试上海财经大学的超时配置
        let timeout = get_timeout_for_url("http://www.shufe.edu.cn/", default);
        assert_eq!(timeout, Duration::from_secs(20));

        // 测试默认超时配置
        let timeout = get_timeout_for_url("http://www.pku.edu.cn/", default);
        assert_eq!(timeout, default);
    }

    #[test]
    fn test_special_handling_detection() {
        assert!(needs_special_handling("http://www.shufe.edu.cn/"));
        assert!(needs_special_handling("http://www.nuaa.edu.cn/"));
        assert!(!needs_special_handling("http://www.pku.edu.cn/"));
    }
}