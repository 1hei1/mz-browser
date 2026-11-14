//! 链式代理（前置代理）
//!
//! 代理配置里带了 `chain_proxy`（前置代理地址，比如本机 Clash 的 `http://127.0.0.1:7890`）时，
//! 先在本机起一个中继端口，再把配置里的 host/port 换成这个端口。之后浏览器、出口检测和
//! 代理测试照常连“代理”，实际链路是：本机中继 → 前置代理 → 目标代理 → 网站。
//! 目标代理的协议和账号密码保持不变，由浏览器/HTTP 客户端自己和目标代理协商。

use runtime::services::environment::kernel::chain_relay::{FrontProxy, ensure_relay};

/// 规范化前置代理设置：空白视为未设置
pub fn normalize_chain_proxy(chain_proxy: Option<&str>) -> Option<&str> {
    chain_proxy.map(str::trim).filter(|value| !value.is_empty())
}

/// 校验前置代理地址格式（保存代理时用）
pub fn validate_chain_proxy(chain_proxy: &str) -> Result<(), String> {
    FrontProxy::parse(chain_proxy).map(|_| ())
}

/// 有前置代理时返回本机中继地址 `("127.0.0.1", 端口)`；没有时返回 None。
pub async fn resolve_chain_endpoint(
    chain_proxy: Option<&str>,
    host: &str,
    port: u16,
) -> Result<Option<(String, u16)>, String> {
    let Some(chain_proxy) = normalize_chain_proxy(chain_proxy) else {
        return Ok(None);
    };
    let front = FrontProxy::parse(chain_proxy)?;
    let relay_port = ensure_relay(&front, host, port).await?;
    Ok(Some(("127.0.0.1".to_string(), relay_port)))
}
