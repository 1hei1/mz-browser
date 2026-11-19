//! 无界面服务器模式（Linux agent 用）：`MZ_HEADLESS=1` 时开启。
//!
//! - 不创建托盘、不登记协议链接；主窗口仍由 Tauri 创建但保持隐藏（需要 Xvfb 之类的 DISPLAY）。
//! - MCP 由环境变量配置并随软件启动，连接信息写到 `<config_dir>/mcp-connection.json`（0600），
//!   本机 agent 读这个文件拿到地址和令牌：
//!   - `MZ_MCP_TOOLS`：开放的工具层，逗号分隔 `page,act,manage,full`，默认 `page,act,manage`
//!   - `MZ_MCP_ALL_ENVIRONMENTS`：默认 1，所有环境都可被页面工具访问（无界面时没法逐个授权）；
//!     0 时只用设置里保存的授权列表
//!   - `MZ_MCP_TOKEN`：64 位十六进制，固定令牌；不设置时沿用/生成保存的令牌

/// 环境变量取值是否为"真"。
pub fn truthy(value: Option<String>) -> Option<bool> {
    value.map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

pub fn enabled() -> bool {
    truthy(std::env::var("MZ_HEADLESS").ok()).unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpOverrides {
    pub page: bool,
    pub act: bool,
    pub manage: bool,
    pub full: bool,
    pub all_environments: bool,
    pub token: Option<String>,
}

impl McpOverrides {
    pub fn from_values(tools: Option<String>, all_environments: Option<String>, token: Option<String>) -> Self {
        let tools = tools.unwrap_or_else(|| "page,act,manage".to_string());
        let has = |name: &str| tools.split(',').any(|t| t.trim().eq_ignore_ascii_case(name));
        let full = has("full");
        let act = has("act") || full;
        let page = has("page") || act;
        Self {
            page,
            act,
            manage: has("manage"),
            full,
            all_environments: truthy(all_environments).unwrap_or(true),
            token: token
                .map(|t| t.trim().to_ascii_lowercase())
                .filter(|t| t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit())),
        }
    }

    pub fn from_env() -> Self {
        Self::from_values(
            std::env::var("MZ_MCP_TOOLS").ok(),
            std::env::var("MZ_MCP_ALL_ENVIRONMENTS").ok(),
            std::env::var("MZ_MCP_TOKEN").ok(),
        )
    }
}

/// 把 MCP 地址和令牌写给本机 agent 读取，只有当前用户可读。
pub fn write_mcp_connection_file(endpoint: &str, token: &str) -> Result<std::path::PathBuf, String> {
    let dir = crate::core::paths::PathManager::get_config_dir().map_err(|e| e.to_string())?;
    let path = dir.join("mcp-connection.json");
    let body = serde_json::json!({
        "endpoint": endpoint,
        "token": token,
        "authorization": format!("Bearer {token}"),
    });
    let text = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::remove_file(&path);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tools_are_page_act_and_manage_on_all_environments() {
        let o = McpOverrides::from_values(None, None, None);
        assert!(o.page && o.act && o.manage && !o.full && o.all_environments);
        assert_eq!(o.token, None);
    }

    #[test]
    fn higher_tiers_pull_in_the_ones_they_need() {
        let o = McpOverrides::from_values(Some("full".into()), Some("0".into()), None);
        assert!(o.full && o.act && o.page && !o.manage && !o.all_environments);
        let o = McpOverrides::from_values(Some(" manage ".into()), None, None);
        assert!(o.manage && !o.page && !o.act);
    }

    #[test]
    fn only_well_formed_tokens_are_taken() {
        let good = "A".repeat(64);
        assert_eq!(McpOverrides::from_values(None, None, Some(good)).token, Some("a".repeat(64)));
        assert_eq!(McpOverrides::from_values(None, None, Some("short".into())).token, None);
    }
}
