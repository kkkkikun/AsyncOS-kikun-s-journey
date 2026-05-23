use axum::{
    body::Body,
    extract::Path as AxumPath,
    extract::State as AxumState,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
};
use anyhow::{Result, Context};

/// 内存中的HTML缓存
#[derive(Clone)]
struct HtmlCache {
    content: Arc<HashMap<String, Arc<String>>>,
}

impl HtmlCache {
    /// 预加载所有HTML文件到内存
    async fn new(cache_dir: PathBuf) -> Result<Self> {
        println!("📂 预加载HTML文件到内存...");

        let mut content_map = HashMap::new();

        // 读取目录中的所有HTML文件
        let mut entries = tokio::fs::read_dir(&cache_dir).await
            .with_context(|| format!("无法读取缓存目录: {:?}", cache_dir))?;

        let mut loaded_count = 0;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("html") {
                continue;
            }

            // 从文件名提取学校名称（移除.html后缀）
            let file_name = path.file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("无效的文件名: {:?}", path))?;

            // 读取文件内容
            let html_content = tokio::fs::read_to_string(&path).await
                .with_context(|| format!("无法读取文件: {:?}", path))?;

            content_map.insert(file_name.to_string(), Arc::new(html_content));
            loaded_count += 1;

            if loaded_count % 10 == 0 {
                println!("   已加载 {}/{} 个文件", loaded_count, content_map.len());
            }
        }

        println!("✅ 预加载完成！{} 个HTML文件已加载到内存", content_map.len());

        Ok(Self {
            content: Arc::new(content_map),
        })
    }

    /// 获取HTML内容
    fn get(&self, school_name: &str) -> Option<Arc<String>> {
        self.content.get(school_name).cloned()
    }
}

/// Mock服务器运行器（在后台运行）
pub struct MockServerRunner {
    base_url: String,
    _handle: tokio::task::JoinHandle<()>,
}

impl MockServerRunner {
    /// 启动Mock服务器并返回运行器
    pub async fn start(cache_dir: PathBuf) -> Result<Self> {
        // 预加载所有HTML文件到内存
        let html_cache = HtmlCache::new(cache_dir.clone()).await?;

        // 构建路由
        let app = Router::new()
            .route("/:school_name", get(handle_request_cached))
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(CorsLayer::permissive())
            )
            .with_state(html_cache);

        // 绑定到随机端口，增加连接队列
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .context("绑定TCP端口失败")?;

        let actual_port = listener.local_addr()
            .context("获取本地地址失败")?
            .port();

        let base_url = format!("http://127.0.0.1:{}", actual_port);
        println!("🚀 Mock服务器已启动: {} (内存缓存模式)", base_url);

        // 启动服务器
        let handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("❌ Mock服务器错误: {}", e);
            }
        });

        Ok(Self {
            base_url,
            _handle: handle,
        })
    }

    /// 获取服务器基础URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// 处理HTTP请求（使用内存缓存）
async fn handle_request_cached(
    AxumPath(school_name): AxumPath<String>,
    AxumState(cache): AxumState<HtmlCache>,
) -> impl IntoResponse {
    match cache.get(&school_name) {
        Some(content) => {
            let mut headers = HeaderMap::new();
            headers.insert("content-type", "text/html; charset=utf-8".parse().unwrap());
            headers.insert("content-length", content.len().to_string().parse().unwrap());
            headers.insert("cache-control", "public, max-age=31536000".parse().unwrap());
            headers.insert("connection", "keep-alive".parse().unwrap());

            // 克隆内容以避免生命周期问题
            let content_clone = (*content).clone();
            (StatusCode::OK, headers, content_clone).into_response()
        }
        None => {
            let error_msg = format!("学校不存在: {} (缓存缺失)", school_name);
            eprintln!("❌ {}", error_msg);
            // 返回503而不是404，模拟服务器过载
            (StatusCode::SERVICE_UNAVAILABLE, error_msg).into_response()
        }
    }
}
