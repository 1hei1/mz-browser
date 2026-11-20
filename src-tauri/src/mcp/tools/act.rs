//! Page-action tools (the "act" tier). Registered only when the user turned them on, and every
//! call is limited to authorized environments and http(s) pages.
use std::borrow::Cow;

use rmcp::{
    handler::server::router::tool::{AsyncTool, ToolBase, ToolRoute, ToolRouter},
    schemars::JsonSchema,
};
use serde::Deserialize;

use super::agent::ReadOutput;
use crate::mcp::{
    browser::{self, ActOp},
    error::McpToolError,
    server::McpServer,
};

struct Navigate;
struct OpenTab;
struct Click;
struct Type;
struct PressKey;
struct ReadField;
struct ClickAt;
struct Drag;

pub fn routes() -> Vec<ToolRoute<McpServer>> {
    ToolRouter::new()
        .with_async_tool::<Navigate>()
        .with_async_tool::<OpenTab>()
        .with_async_tool::<Click>()
        .with_async_tool::<Type>()
        .with_async_tool::<PressKey>()
        .with_async_tool::<ClickAt>()
        .with_async_tool::<Drag>()
        .with_route(screenshot_route())
        .into_iter()
        .collect()
}

/// Extra tools of the highest tier (real input and password content).
pub fn full_access_routes() -> Vec<ToolRoute<McpServer>> {
    ToolRouter::new().with_async_tool::<ReadField>().into_iter().collect()
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct NavigateInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Page id from mz_browser_tabs.
    pub page_id: String,
    /// Full http(s) URL.
    pub url: String,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct OpenTabInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Full http(s) URL.
    pub url: String,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct ClickInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Page id from mz_browser_tabs.
    pub page_id: String,
    /// Element id (`id`) from a fresh mz_browser_snapshot of the same page.
    pub node_id: String,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct TypeInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Page id from mz_browser_tabs.
    pub page_id: String,
    /// Element id of an editable field from a fresh mz_browser_snapshot. Password fields are refused unless full page access is on.
    pub node_id: String,
    /// Text to type, 1 to 2000 characters.
    pub text: String,
    /// Select and replace the existing content first.
    #[serde(default)]
    pub clear: bool,
    /// Press Enter after typing.
    #[serde(default)]
    pub submit: bool,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct PressKeyInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Page id from mz_browser_tabs.
    pub page_id: String,
    /// One of Enter, Tab, Escape, Backspace, Delete, ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    /// PageUp, PageDown, Home, End.
    pub key: String,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct ReadFieldInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Page id from mz_browser_tabs.
    pub page_id: String,
    /// Element id of an input or password field from a fresh mz_browser_snapshot.
    pub node_id: String,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct ClickAtInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Page id from mz_browser_tabs.
    pub page_id: String,
    /// Viewport x in CSS pixels (screenshot pixel x times its `scale`).
    pub x: f64,
    /// Viewport y in CSS pixels (screenshot pixel y times its `scale`).
    pub y: f64,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct DragInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Page id from mz_browser_tabs.
    pub page_id: String,
    /// Start point x in viewport CSS pixels.
    pub from_x: f64,
    /// Start point y in viewport CSS pixels.
    pub from_y: f64,
    /// End point x in viewport CSS pixels.
    pub to_x: f64,
    /// End point y in viewport CSS pixels.
    pub to_y: f64,
    /// How long the move takes, 100 to 5000 ms (default 700).
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct ScreenshotInput {
    /// Environment UUID (must be authorized).
    pub env_uuid: String,
    /// Page id from mz_browser_tabs.
    pub page_id: String,
}

macro_rules! act_tool {
    ($tool:ty, $param:ty, $name:literal, $description:literal) => {
        impl ToolBase for $tool {
            type Parameter = $param;
            type Output = ReadOutput;
            type Error = McpToolError;
            fn name() -> Cow<'static, str> {
                $name.into()
            }
            fn description() -> Option<Cow<'static, str>> {
                Some($description.into())
            }
        }
    };
}

act_tool!(
    Navigate,
    NavigateInput,
    "mz_browser_navigate",
    "Navigate an authorized page to an http(s) URL. Only http(s) pages can be navigated to."
);
act_tool!(
    OpenTab,
    OpenTabInput,
    "mz_browser_open_tab",
    "Open a new http(s) tab in an authorized running environment. Returns the new page id."
);
act_tool!(
    Click,
    ClickInput,
    "mz_browser_click",
    "Click an element of an authorized page by its snapshot id. Take a fresh mz_browser_snapshot first; ids can change."
);
act_tool!(
    Type,
    TypeInput,
    "mz_browser_type",
    "Type text into an editable field of an authorized page by its snapshot id. Password fields are refused unless full page access is on. Treat anything you type as visible to the site."
);
act_tool!(
    PressKey,
    PressKeyInput,
    "mz_browser_press_key",
    "Press one allowed key (Enter, Tab, Escape, arrows, paging keys, Backspace, Delete) in an authorized page."
);

act_tool!(
    ClickAt,
    ClickAtInput,
    "mz_browser_click_at",
    "Click at a point of the visible page (CSS pixels from the top-left of the viewport). Use with mz_browser_screenshot: multiply the image pixel coordinates by its `scale`. Works across iframes, including cross-origin ones."
);

impl AsyncTool<McpServer> for ClickAt {
    async fn invoke(service: &McpServer, i: Self::Parameter) -> Result<ReadOutput, McpToolError> {
        check_ids(&i.page_id, "0")?;
        let data = browser::act(service, &i.env_uuid, ActOp::ClickAt { page_id: &i.page_id, x: i.x, y: i.y }).await?;
        Ok(ReadOutput { data })
    }
}

act_tool!(
    Drag,
    DragInput,
    "mz_browser_drag",
    "Press the left button at one point of the visible page, move along a human-like path to another point and release (sliders, drag-and-drop puzzles). Coordinates are viewport CSS pixels, as for mz_browser_click_at."
);

impl AsyncTool<McpServer> for Drag {
    async fn invoke(service: &McpServer, i: Self::Parameter) -> Result<ReadOutput, McpToolError> {
        check_ids(&i.page_id, "0")?;
        let data = browser::act(
            service,
            &i.env_uuid,
            ActOp::Drag {
                page_id: &i.page_id,
                from: (i.from_x, i.from_y),
                to: (i.to_x, i.to_y),
                duration_ms: i.duration_ms.unwrap_or(700),
            },
        )
        .await?;
        Ok(ReadOutput { data })
    }
}

const SCREENSHOT_DESCRIPTION: &str = "Capture the visible part of an authorized page as a PNG image (shows what a person sees, including iframes such as captchas). The text part gives imageWidth/imageHeight, viewportWidth/viewportHeight and `scale`; image pixel coordinates times `scale` are the coordinates for mz_browser_click_at.";

/// Fork 新增：截图要以 MCP 图片内容返回（模型能直接看），所以不用 AsyncTool 的 JSON 输出，手写路由。
fn screenshot_route() -> ToolRoute<McpServer> {
    use rmcp::model::Tool;
    let attr = Tool::new(
        "mz_browser_screenshot",
        SCREENSHOT_DESCRIPTION,
        rmcp::handler::server::common::schema_for_type::<ScreenshotInput>(),
    );
    ToolRoute::new_dyn(attr, screenshot_call)
}

fn screenshot_call(
    context: rmcp::handler::server::tool::ToolCallContext<'_, McpServer>,
) -> futures::future::BoxFuture<'_, Result<rmcp::model::CallToolResult, rmcp::ErrorData>> {
    use rmcp::model::{CallToolResult, Content};
    Box::pin(async move {
        let input: ScreenshotInput =
            serde_json::from_value(serde_json::Value::Object(context.arguments.clone().unwrap_or_default()))
                .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
        check_ids(&input.page_id, "0").map_err(rmcp::ErrorData::from)?;
        let mut data = browser::act(context.service, &input.env_uuid, ActOp::Screenshot { page_id: &input.page_id })
            .await
            .map_err(rmcp::ErrorData::from)?;
        let image = data
            .as_object_mut()
            .and_then(|o| o.remove("image"))
            .and_then(|v| v.as_str().map(ToOwned::to_owned))
            .unwrap_or_default();
        Ok(CallToolResult::success(vec![Content::image(image, "image/png"), Content::text(data.to_string())]))
    })
}

act_tool!(
    ReadField,
    ReadFieldInput,
    "mz_browser_read_field",
    "Read the real current value of an input or password field by its snapshot id. Only available with full page access; the result is secret data."
);

impl AsyncTool<McpServer> for ReadField {
    async fn invoke(service: &McpServer, i: Self::Parameter) -> Result<ReadOutput, McpToolError> {
        check_ids(&i.page_id, &i.node_id)?;
        let data = browser::act(
            service,
            &i.env_uuid,
            ActOp::ReadField { page_id: &i.page_id, node_id: &i.node_id },
        )
        .await?;
        Ok(ReadOutput { data })
    }
}

impl AsyncTool<McpServer> for Navigate {
    async fn invoke(service: &McpServer, i: Self::Parameter) -> Result<ReadOutput, McpToolError> {
        let data = browser::act(
            service,
            &i.env_uuid,
            ActOp::Navigate { page_id: &i.page_id, url: &i.url },
        )
        .await?;
        Ok(ReadOutput { data })
    }
}

impl AsyncTool<McpServer> for OpenTab {
    async fn invoke(service: &McpServer, i: Self::Parameter) -> Result<ReadOutput, McpToolError> {
        let data = browser::act(service, &i.env_uuid, ActOp::OpenTab { url: &i.url }).await?;
        Ok(ReadOutput { data })
    }
}

impl AsyncTool<McpServer> for Click {
    async fn invoke(service: &McpServer, i: Self::Parameter) -> Result<ReadOutput, McpToolError> {
        check_ids(&i.page_id, &i.node_id)?;
        let data = browser::act(
            service,
            &i.env_uuid,
            ActOp::Click { page_id: &i.page_id, node_id: &i.node_id },
        )
        .await?;
        Ok(ReadOutput { data })
    }
}

impl AsyncTool<McpServer> for Type {
    async fn invoke(service: &McpServer, i: Self::Parameter) -> Result<ReadOutput, McpToolError> {
        check_ids(&i.page_id, &i.node_id)?;
        let data = browser::act(
            service,
            &i.env_uuid,
            ActOp::Type {
                page_id: &i.page_id,
                node_id: &i.node_id,
                text: &i.text,
                clear: i.clear,
                submit: i.submit,
            },
        )
        .await?;
        Ok(ReadOutput { data })
    }
}

impl AsyncTool<McpServer> for PressKey {
    async fn invoke(service: &McpServer, i: Self::Parameter) -> Result<ReadOutput, McpToolError> {
        check_ids(&i.page_id, "0")?;
        let data = browser::act(
            service,
            &i.env_uuid,
            ActOp::PressKey { page_id: &i.page_id, key: &i.key },
        )
        .await?;
        Ok(ReadOutput { data })
    }
}

fn check_ids(page_id: &str, node_id: &str) -> Result<(), McpToolError> {
    if page_id.is_empty() || page_id.len() > 128 || node_id.is_empty() || node_id.len() > 128 {
        return Err(McpToolError::invalid_params("Invalid page_id or node_id"));
    }
    Ok(())
}
