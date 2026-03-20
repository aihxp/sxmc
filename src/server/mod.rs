//! MCP server construction and transport serving.
//!
//! This module is the main bridge from parsed skills to runnable MCP servers.
//! It supports both local stdio serving and remote streamable HTTP serving.

pub mod handler;

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::Request,
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, tower::StreamableHttpService, StreamableHttpServerConfig,
};
use tokio_util::sync::CancellationToken;

use crate::error::{Result, SxmcError};
use crate::skills::discovery;
use crate::skills::parser;

use self::handler::SkillsServer;

#[derive(Clone, Debug)]
pub struct HttpAuth {
    header_name: HeaderName,
    header_value: HeaderValue,
}

#[derive(Clone, Debug, Default)]
struct HttpAuthConfig {
    rules: Vec<HttpAuth>,
    schemes: Vec<&'static str>,
}

#[derive(Clone, Debug)]
struct HttpServerInfo {
    auth_enabled: bool,
    auth_schemes: Vec<&'static str>,
    skill_count: usize,
    tool_count: usize,
    resource_count: usize,
}

#[derive(Clone, Debug, Default)]
struct SkillInventorySummary {
    skill_count: usize,
    tool_count: usize,
    resource_count: usize,
}

impl HttpAuth {
    fn try_from_pair(header_name: &str, header_value: &str) -> Result<Self> {
        let header_name = header_name.parse::<HeaderName>().map_err(|e| {
            SxmcError::Other(format!("Invalid required header name '{header_name}': {e}"))
        })?;
        let header_value = header_value.parse::<HeaderValue>().map_err(|e| {
            SxmcError::Other(format!(
                "Invalid required header value for '{}': {e}",
                header_name
            ))
        })?;

        Ok(Self {
            header_name,
            header_value,
        })
    }

    fn matches(&self, headers: &HeaderMap) -> bool {
        headers
            .get(&self.header_name)
            .is_some_and(|value| value == self.header_value)
    }
}

impl HttpAuthConfig {
    fn new(required_headers: &[(String, String)], bearer_token: Option<&str>) -> Result<Self> {
        let mut rules = required_headers
            .iter()
            .map(|(name, value)| HttpAuth::try_from_pair(name, value))
            .collect::<Result<Vec<_>>>()?;
        let mut schemes = Vec::new();

        if !required_headers.is_empty() {
            schemes.push("headers");
        }

        if let Some(token) = bearer_token {
            rules.push(HttpAuth::try_from_pair(
                "Authorization",
                &format!("Bearer {token}"),
            )?);
            schemes.push("bearer");
        }

        Ok(Self { rules, schemes })
    }

    fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    fn bearer_enabled(&self) -> bool {
        self.schemes.contains(&"bearer")
    }
}

async fn require_auth(auth: Arc<HttpAuthConfig>, request: Request, next: Next) -> Response {
    if auth
        .rules
        .iter()
        .all(|required| required.matches(request.headers()))
    {
        next.run(request).await
    } else {
        let mut response = (StatusCode::UNAUTHORIZED, "Unauthorized\n").into_response();
        if auth.bearer_enabled() {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"sxmc\""),
            );
        }
        response
    }
}

async fn root_handler() -> &'static str {
    "sxmc streamable HTTP MCP server\nEndpoint: /mcp\nHealth: /healthz\n"
}

async fn health_handler(info: Arc<HttpServerInfo>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "sxmc",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "ok",
        "transport": "streamable-http",
        "endpoint": "/mcp",
        "auth": {
            "enabled": info.auth_enabled,
            "schemes": info.auth_schemes,
        },
        "inventory": {
            "skills": info.skill_count,
            "tools": info.tool_count,
            "resources": info.resource_count,
        }
    }))
}

fn summarize_paths(paths: &[PathBuf]) -> SkillInventorySummary {
    let Ok(server) = build_server(paths) else {
        return SkillInventorySummary::default();
    };

    let skills = server.skills();
    SkillInventorySummary {
        skill_count: skills.len(),
        tool_count: skills.iter().map(|s| s.scripts.len()).sum(),
        resource_count: skills.iter().map(|s| s.references.len()).sum(),
    }
}

/// Build a SkillsServer from skill search paths.
pub fn build_server(paths: &[PathBuf]) -> Result<SkillsServer> {
    let skill_dirs = discovery::discover_skills(paths)?;
    let mut skills = Vec::new();

    for dir in &skill_dirs {
        let source = dir.parent().and_then(|p| p.to_str()).unwrap_or("unknown");
        match parser::parse_skill(dir, source) {
            Ok(skill) => {
                eprintln!("[sxmc] Loaded skill: {}", skill.name);
                skills.push(skill);
            }
            Err(e) => {
                eprintln!("[sxmc] Warning: failed to parse {}: {}", dir.display(), e);
            }
        }
    }

    eprintln!(
        "[sxmc] Loaded {} skills with {} tools and {} resources",
        skills.len(),
        skills.iter().map(|s| s.scripts.len()).sum::<usize>(),
        skills.iter().map(|s| s.references.len()).sum::<usize>(),
    );

    Ok(SkillsServer::new(skills))
}

/// Run the MCP server over stdio.
pub async fn serve_stdio(paths: &[PathBuf]) -> Result<()> {
    let server = build_server(paths)?;
    let transport = rmcp::transport::stdio();

    let service = rmcp::ServiceExt::serve(server, transport)
        .await
        .map_err(|e| crate::error::SxmcError::McpError(e.to_string()))?;

    service
        .waiting()
        .await
        .map_err(|e| crate::error::SxmcError::McpError(e.to_string()))?;

    Ok(())
}

fn build_streamable_http_service(
    paths: Arc<Vec<PathBuf>>,
    cancellation_token: CancellationToken,
) -> StreamableHttpService<SkillsServer, LocalSessionManager> {
    StreamableHttpService::new(
        move || build_server(&paths).map_err(std::io::Error::other),
        Default::default(),
        StreamableHttpServerConfig {
            stateful_mode: true,
            json_response: false,
            cancellation_token,
            ..Default::default()
        },
    )
}

fn build_http_router(
    paths: Arc<Vec<PathBuf>>,
    cancellation_token: CancellationToken,
    auth: Arc<HttpAuthConfig>,
) -> Router {
    let summary = summarize_paths(paths.as_ref());
    let service = build_streamable_http_service(paths, cancellation_token);
    let info = Arc::new(HttpServerInfo {
        auth_enabled: !auth.is_empty(),
        auth_schemes: auth.schemes.clone(),
        skill_count: summary.skill_count,
        tool_count: summary.tool_count,
        resource_count: summary.resource_count,
    });
    let mcp_router = Router::new().nest_service("/mcp", service);
    let mcp_router = if auth.is_empty() {
        mcp_router
    } else {
        mcp_router.layer(middleware::from_fn({
            let auth = auth.clone();
            move |request, next| require_auth(auth.clone(), request, next)
        }))
    };

    Router::new()
        .route("/", get(root_handler))
        .route(
            "/healthz",
            get({
                let info = info.clone();
                move || health_handler(info.clone())
            }),
        )
        .merge(mcp_router)
}

/// Run the MCP server over streamable HTTP.
///
/// When `required_headers` is non-empty, every request to `/mcp` must include
/// all configured header/value pairs.
pub async fn serve_http(
    paths: &[PathBuf],
    host: &str,
    port: u16,
    required_headers: &[(String, String)],
    bearer_token: Option<&str>,
) -> Result<()> {
    let bind_addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| crate::error::SxmcError::Other(format!("Failed to bind {bind_addr}: {e}")))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| crate::error::SxmcError::Other(format!("Failed to read local addr: {e}")))?;
    let cancellation_token = CancellationToken::new();
    let auth = HttpAuthConfig::new(required_headers, bearer_token)?;
    let router = build_http_router(
        Arc::new(paths.to_vec()),
        cancellation_token.clone(),
        Arc::new(auth),
    );

    eprintln!(
        "[sxmc] Streamable HTTP MCP server listening at http://{}/mcp",
        local_addr
    );
    if !required_headers.is_empty() {
        eprintln!(
            "[sxmc] Remote MCP auth enabled with {} required header(s)",
            required_headers.len()
        );
    }
    if bearer_token.is_some() {
        eprintln!("[sxmc] Bearer token auth enabled for remote MCP access");
    }

    let shutdown = cancellation_token.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown.cancel();
    });

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            cancellation_token.cancelled_owned().await;
        })
        .await
        .map_err(|e| crate::error::SxmcError::Other(format!("HTTP server failed: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, HeaderValue, StatusCode};

    #[tokio::test]
    async fn test_streamable_http_server_serves_mcp_endpoint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let router = build_http_router(
            Arc::new(vec![PathBuf::from("tests/fixtures")]),
            cancel.child_token(),
            Arc::new(HttpAuthConfig::default()),
        );

        let handle = tokio::spawn({
            let cancel = cancel.clone();
            async move {
                let _ = axum::serve(listener, router)
                    .with_graceful_shutdown(async move {
                        cancel.cancelled_owned().await;
                    })
                    .await;
            }
        });

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/mcp"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(content_type.contains("text/event-stream"));

        cancel.cancel();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_streamable_http_server_requires_auth_header() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let router = build_http_router(
            Arc::new(vec![PathBuf::from("tests/fixtures")]),
            cancel.child_token(),
            Arc::new(HttpAuthConfig::new(&[], Some("test-token")).unwrap()),
        );

        let handle = tokio::spawn({
            let cancel = cancel.clone();
            async move {
                let _ = axum::serve(listener, router)
                    .with_graceful_shutdown(async move {
                        cancel.cancelled_owned().await;
                    })
                    .await;
            }
        });

        let client = reqwest::Client::new();
        let unauthorized = client
            .post(format!("http://{addr}/mcp"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            unauthorized
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok()),
            Some("Bearer realm=\"sxmc\"")
        );

        let authorized = client
            .post(format!("http://{addr}/mcp"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Authorization", HeaderValue::from_static("Bearer test-token"))
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);

        cancel.cancel();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_health_endpoint_reports_auth_modes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let router = build_http_router(
            Arc::new(vec![PathBuf::from("tests/fixtures")]),
            cancel.child_token(),
            Arc::new(
                HttpAuthConfig::new(
                    &[("X-API-Key".to_string(), "abc123".to_string())],
                    Some("test-token"),
                )
                .unwrap(),
            ),
        );

        let handle = tokio::spawn({
            let cancel = cancel.clone();
            async move {
                let _ = axum::serve(listener, router)
                    .with_graceful_shutdown(async move {
                        cancel.cancelled_owned().await;
                    })
                    .await;
            }
        });

        let response = reqwest::get(format!("http://{addr}/healthz"))
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();

        assert_eq!(response["status"], "ok");
        assert_eq!(response["endpoint"], "/mcp");
        assert_eq!(response["auth"]["enabled"], true);
        assert_eq!(
            response["auth"]["schemes"],
            serde_json::json!(["headers", "bearer"])
        );
        assert_eq!(response["inventory"]["skills"], 4);
        assert_eq!(response["inventory"]["tools"], 1);
        assert_eq!(response["inventory"]["resources"], 1);

        cancel.cancel();
        handle.await.unwrap();
    }
}
