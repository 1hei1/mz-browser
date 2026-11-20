use serde_json::Value;
use std::time::Duration;

use super::{bidi_page, cdp, cdp_act, error::McpToolError, server::McpServer};

/// No per-site list: every http(s) page of an authorized environment is in scope.
const ANY_SITE: &[String] = &[];

/// Reads tabs (`page_id == None`) or one page snapshot from an authorized, running environment.
pub(crate) async fn read_page(
    service: &McpServer,
    env_uuid: &str,
    page_id: Option<&str>,
) -> Result<Value, McpToolError> {
    service.require_environment(env_uuid).await?;
    let _slot = service
        .config()
        .read_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| McpToolError::upstream("Browser read capacity reached; retry later"))?;
    if !service.config().access.page_tools_enabled {
        return Err(McpToolError::invalid_params("Page tools are disabled"));
    }
    let origins = ANY_SITE;
    let endpoint = service
        .bridge()
        .get_environment_cdp_endpoint(env_uuid)
        .await?
        .and_then(|endpoint| endpoint.browser_ws_url)
        .ok_or_else(|| McpToolError::invalid_params("Environment is not running"))?;
    let action = if page_id.is_some() { "snapshot" } else { "tabs" };
    let full_access = service.config().access.full_page_access_enabled;
    // Firefox 环境给的是共用 BiDi 会话的地址，Chromium 环境是 CDP 地址
    let read = async {
        if bidi_page::is_gecko_endpoint(&endpoint) {
            bidi_page::read(&endpoint, origins, page_id, full_access).await
        } else {
            cdp::read(&endpoint, origins, page_id, full_access).await
        }
    };
    let result = tokio::time::timeout(Duration::from_secs(20), read)
    .await
    .map_err(|_| McpToolError::upstream("Browser read timed out"))
    .and_then(|result| result);
    log::info!(
        "MCP browser read env={} action={} success={}",
        env_uuid,
        action,
        result.is_ok()
    );
    let data = result?;
    // Recheck session and authorization before releasing page content.
    service.require_environment(env_uuid).await?;
    Ok(data)
}

/// One page action of the "act" tier.
pub(crate) enum ActOp<'a> {
    Navigate { page_id: &'a str, url: &'a str },
    OpenTab { url: &'a str },
    Click { page_id: &'a str, node_id: &'a str },
    Type { page_id: &'a str, node_id: &'a str, text: &'a str, clear: bool, submit: bool },
    PressKey { page_id: &'a str, key: &'a str },
    ReadField { page_id: &'a str, node_id: &'a str },
    /// Fork 新增：按视口坐标点击、可见区域截图
    ClickAt { page_id: &'a str, x: f64, y: f64 },
    Screenshot { page_id: &'a str },
    Drag { page_id: &'a str, from: (f64, f64), to: (f64, f64), duration_ms: u64 },
}

impl ActOp<'_> {
    fn name(&self) -> &'static str {
        match self {
            Self::Navigate { .. } => "navigate",
            Self::OpenTab { .. } => "open_tab",
            Self::Click { .. } => "click",
            Self::Type { .. } => "type",
            Self::PressKey { .. } => "press_key",
            Self::ReadField { .. } => "read_field",
            Self::ClickAt { .. } => "click_at",
            Self::Screenshot { .. } => "screenshot",
            Self::Drag { .. } => "drag",
        }
    }
}

/// Runs a page action in an authorized, running environment (http/https pages only).
pub(crate) async fn act(
    service: &McpServer,
    env_uuid: &str,
    op: ActOp<'_>,
) -> Result<Value, McpToolError> {
    if matches!(op, ActOp::ReadField { .. }) {
        service.require_full_access().await?;
    } else {
        service.require_act().await?;
    }
    service.require_environment(env_uuid).await?;
    let _slot = service
        .config()
        .read_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| McpToolError::upstream("Browser capacity reached; retry later"))?;
    let origins = ANY_SITE;
    let endpoint = service
        .bridge()
        .get_environment_cdp_endpoint(env_uuid)
        .await?
        .and_then(|endpoint| endpoint.browser_ws_url)
        .ok_or_else(|| McpToolError::invalid_params("Environment is not running"))?;
    let name = op.name();
    let full_access = service.config().access.full_page_access_enabled;
    if bidi_page::is_gecko_endpoint(&endpoint) {
        let run_gecko = async {
            let mut page = bidi_page::BidiPage::connect(&endpoint, full_access).await?;
            let result = match op {
                ActOp::Navigate { page_id, url } => bidi_page::navigate(&mut page, origins, page_id, url).await,
                ActOp::OpenTab { url } => bidi_page::open_tab(&mut page, origins, url).await,
                ActOp::Click { page_id, node_id } => bidi_page::click(&mut page, origins, page_id, node_id).await,
                ActOp::Type { page_id, node_id, text, clear, submit } => {
                    bidi_page::type_text(&mut page, origins, page_id, node_id, text, clear, submit).await
                }
                ActOp::PressKey { page_id, key } => bidi_page::press_key(&mut page, origins, page_id, key).await,
                ActOp::ReadField { page_id, node_id } => bidi_page::read_field(&mut page, origins, page_id, node_id).await,
                ActOp::ClickAt { .. } | ActOp::Screenshot { .. } | ActOp::Drag { .. } => {
                    Err(McpToolError::invalid_params("Not supported for Firefox environments"))
                }
            };
            page.close().await;
            result
        };
        let result = tokio::time::timeout(Duration::from_secs(20), run_gecko)
            .await
            .map_err(|_| McpToolError::upstream("Browser action timed out"))
            .and_then(|result| result);
        log::info!("MCP browser act env={} action={} success={}", env_uuid, name, result.is_ok());
        let data = result?;
        service.require_environment(env_uuid).await?;
        return Ok(data);
    }
    let run = async {
        let mut conn = cdp::CdpConnection::connect(&endpoint)
            .await?
            .with_full_access(service.config().access.full_page_access_enabled);
        let result = match op {
            ActOp::Navigate { page_id, url } => {
                cdp_act::navigate(&mut conn, origins, page_id, url).await
            }
            ActOp::OpenTab { url } => cdp_act::open_tab(&mut conn, origins, url).await,
            ActOp::Click { page_id, node_id } => {
                cdp_act::click(&mut conn, origins, page_id, node_id).await
            }
            ActOp::Type { page_id, node_id, text, clear, submit } => {
                cdp_act::type_text(&mut conn, origins, page_id, node_id, text, clear, submit).await
            }
            ActOp::PressKey { page_id, key } => {
                cdp_act::press_key(&mut conn, origins, page_id, key).await
            }
            ActOp::ReadField { page_id, node_id } => {
                cdp_act::read_field(&mut conn, origins, page_id, node_id).await
            }
            ActOp::ClickAt { page_id, x, y } => cdp_act::click_at(&mut conn, origins, page_id, x, y).await,
            ActOp::Screenshot { page_id } => cdp_act::screenshot(&mut conn, origins, page_id).await,
            ActOp::Drag { page_id, from, to, duration_ms } => {
                cdp_act::drag(&mut conn, origins, page_id, from, to, duration_ms).await
            }
        };
        conn.close().await;
        result
    };
    let result = tokio::time::timeout(Duration::from_secs(20), run)
        .await
        .map_err(|_| McpToolError::upstream("Browser action timed out"))
        .and_then(|result| result);
    // never log typed text or full URLs
    log::info!("MCP browser act env={} action={} success={}", env_uuid, name, result.is_ok());
    let data = result?;
    service.require_environment(env_uuid).await?;
    Ok(data)
}
