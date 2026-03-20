//! MCP server construction and transport serving.
//!
//! This module is the main bridge from parsed skills to runnable MCP servers.
//! It supports both local stdio serving and remote streamable HTTP serving.

pub mod handler;

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
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

async fn require_auth(
    State(auth): State<Arc<Vec<HttpAuth>>>,
    request: Request,
    next: Next,
) -> Response {
    if auth
        .iter()
        .all(|required| required.matches(request.headers()))
    {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, "Unauthorized\n").into_response()
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
    auth: Arc<Vec<HttpAuth>>,
) -> Router {
    let service = build_streamable_http_service(paths, cancellation_token);
    let mcp_router = Router::new().nest_service("/mcp", service);
    let mcp_router = if auth.is_empty() {
        mcp_router
    } else {
        mcp_router.layer(middleware::from_fn_with_state(auth, require_auth))
    };

    Router::new()
        .route(
            "/",
            get(|| async { "sxmc streamable HTTP MCP server\nEndpoint: /mcp\n" }),
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
) -> Result<()> {
    let bind_addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| crate::error::SxmcError::Other(format!("Failed to bind {bind_addr}: {e}")))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| crate::error::SxmcError::Other(format!("Failed to read local addr: {e}")))?;
    let cancellation_token = CancellationToken::new();
    let auth = required_headers
        .iter()
        .map(|(name, value)| HttpAuth::try_from_pair(name, value))
        .collect::<Result<Vec<_>>>()?;
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
    use axum::http::{HeaderValue, StatusCode};

    #[tokio::test]
    async fn test_streamable_http_server_serves_mcp_endpoint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let router = build_http_router(
            Arc::new(vec![PathBuf::from("tests/fixtures")]),
            cancel.child_token(),
            Arc::new(Vec::new()),
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
            Arc::new(vec![HttpAuth::try_from_pair(
                "Authorization",
                "Bearer test-token",
            )
            .unwrap()]),
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
}
