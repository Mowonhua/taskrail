//! 文件职责：将 OAuth 已验证身份传给进程内的专用 Agent 路由。
use super::models::AgentSummary;
use crate::agent_tools::{Operation, ToolResult};
use axum::{
    body::{to_bytes, Body},
    http::{header, Request},
    Router,
};
use serde_json::json;
use tower::ServiceExt;

/// 该扩展只能由服务端鉴权层构造，OAuth 令牌不会转发给 REST API。
#[derive(Clone)]
pub(crate) struct McpIdentity(pub(super) AgentSummary);

impl McpIdentity {
    pub(crate) async fn execute(&self, api: Router, operation: Operation) -> ToolResult {
        let mut request = Request::builder()
            .method(operation.method)
            .uri(operation.path)
            .extension(self.clone());
        let body = if let Some(value) = operation.body {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        } else {
            Body::empty()
        };
        let failure =
            || json!({"error":{"code":"INTERNAL_ERROR","message":"无法读取 Taskrail 工具结果"}});
        let request = request.body(body).map_err(|_| failure())?;
        let response = api.oneshot(request).await.map_err(|_| failure())?;
        let success = response.status().is_success();
        let bytes = to_bytes(response.into_body(), 32 * 1024 * 1024)
            .await
            .map_err(|_| failure())?;
        let value = serde_json::from_slice(&bytes).map_err(|_| failure())?;
        if success {
            Ok(value)
        } else {
            Err(value)
        }
    }
}
