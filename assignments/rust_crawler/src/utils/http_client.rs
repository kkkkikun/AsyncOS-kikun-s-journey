use anyhow::{Result, Context};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, ACCEPT, ACCEPT_LANGUAGE, ACCEPT_ENCODING, CONNECTION, CACHE_CONTROL, REFERER};
use std::time::Duration;

/// HTTP客户端配置
#[derive(Clone)]
pub struct HttpClientConfig {
    pub timeout: Duration,
    pub max_retries: usize,
    pub retry_delay: Duration,
    pub user_agent: Option<String>,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_retries: 3,
            retry_delay: Duration::from_millis(500),
            user_agent: Some(default_user_agent()),
        }
    }
}

impl HttpClientConfig {
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }
}

/// 创建默认的User-Agent
fn default_user_agent() -> String {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36".to_string()
}

/// 创建带有默认headers的HeaderMap（更完整，避免412错误）
pub fn create_default_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();

    // User-Agent
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
    );

    // Accept
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7")
    );

    // Accept-Language
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8,ja;q=0.7")
    );

    // Accept-Encoding
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br")
    );

    // Connection
    headers.insert(
        CONNECTION,
        HeaderValue::from_static("keep-alive")
    );

    // Cache-Control
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("max-age=0")
    );

    // Referer (设置为空，避免某些网站的防盗链)
    headers.insert(
        REFERER,
        HeaderValue::from_static("")
    );

    // Upgrade-Insecure-Requests
    headers.insert("Upgrade-Insecure-Requests", HeaderValue::from_static("1"));

    // Sec-Fetch-Dest
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("document"));

    // Sec-Fetch-Mode
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("navigate"));

    // Sec-Fetch-Site
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("none"));

    // Sec-Fetch-User
    headers.insert("Sec-Fetch-User", HeaderValue::from_static("?1"));

    headers
}

/// 带重试的HTTP GET请求
pub async fn http_get_with_retry(
    client: &reqwest::Client,
    url: &str,
    config: &HttpClientConfig,
) -> Result<reqwest::Response> {
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 0..config.max_retries {
        match client.get(url).send().await {
            Ok(response) => {
                let status = response.status();

                // 检查是否需要重试（412, 403, 503等错误可以重试）
                if status.is_success() {
                    return Ok(response);
                } else if is_retryable_error(status.as_u16()) && attempt < config.max_retries - 1 {
                    eprintln!("⚠️  [尝试 {}/{}] HTTP错误: {}，将重试...",
                        attempt + 1, config.max_retries, status);
                    tokio::time::sleep(config.retry_delay).await;
                    continue;
                } else {
                    return Ok(response);
                }
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!(e));

                // 如果是网络错误或超时，可以重试
                if attempt < config.max_retries - 1 {
                    eprintln!("⚠️  [尝试 {}/{}] 请求失败: {}，将重试...",
                        attempt + 1, config.max_retries, last_error.as_ref().unwrap());
                    tokio::time::sleep(config.retry_delay).await;
                    continue;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("请求失败")))
}

/// 判断是否为可重试的HTTP错误
fn is_retryable_error(status_code: u16) -> bool {
    matches!(status_code,
        408 | // Request Timeout
        429 | // Too Many Requests
        500 | // Internal Server Error
        502 | // Bad Gateway
        503 | // Service Unavailable
        504   // Gateway Timeout
    )
}

/// 为同步客户端创建builder（用于线程爬虫和进程爬虫）
pub fn create_blocking_client_builder(timeout: Duration) -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .default_headers(create_default_headers())
        .connect_timeout(Duration::from_secs(5))
        .pool_max_idle_per_host(10)
        .no_proxy() // 🔥 关键修复：禁用代理，避免Mock服务器请求被系统代理拦截
}

/// 为异步客户端创建builder（用于协程爬虫）
pub fn create_async_client_builder(timeout: Duration) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(timeout)
        .default_headers(create_default_headers())
        .connect_timeout(Duration::from_secs(5))
        .pool_max_idle_per_host(10)
        .no_proxy() // 🔥 关键修复：禁用代理，避免Mock服务器请求被系统代理拦截
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_headers() {
        let headers = create_default_headers();
        assert!(headers.contains_key(USER_AGENT));
        assert!(headers.contains_key(ACCEPT));
        assert!(headers.contains_key(ACCEPT_LANGUAGE));
    }

    #[test]
    fn test_retryable_errors() {
        assert!(is_retryable_error(503));
        assert!(is_retryable_error(504));
        assert!(is_retryable_error(429));
        assert!(!is_retryable_error(404));
        assert!(!is_retryable_error(400));
    }
}
