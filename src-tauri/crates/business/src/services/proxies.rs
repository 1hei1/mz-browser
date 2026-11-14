use uuid::Uuid;

use crate::dto::ProxyDto;
use crate::entitys::{
    BatchImportProxiesRequest, BatchSetChainProxyRequest, CreateProxyRequest, ListProxiesRequest,
    UpdateProxyRequest,
};
use crate::models;
use crate::svc_ctx::SvcCtx;

/// 校验并规范化前置代理地址（链式代理）。
///
/// 只接受 `http://host:port` 或 `socks5://[user:pass@]host:port`（不写协议按 http）；
/// 空白返回 `None`。
pub fn normalize_chain_proxy(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let rest = match value.split_once("://") {
        Some((scheme, rest)) => {
            if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "socks5" | "socks5h" | "socks") {
                return Err(format!("前置代理只支持 http 和 socks5 协议：{value}"));
            }
            rest
        }
        None => value,
    };
    let address = rest.trim_end_matches('/').rsplit_once('@').map_or(rest.trim_end_matches('/'), |(_, address)| address);
    let valid = address.rsplit_once(':').is_some_and(|(host, port)| {
        !host.trim_start_matches('[').trim_end_matches(']').trim().is_empty()
            && port.trim().parse::<u16>().is_ok_and(|port| port != 0)
    });
    if !valid {
        return Err(format!("前置代理地址格式应为 协议://主机:端口，例如 http://127.0.0.1:7890：{value}"));
    }
    Ok(Some(value.to_string()))
}

/// 创建代理
pub async fn create_proxy_service(
    svc_ctx: &SvcCtx,
    payload: &CreateProxyRequest,
) -> Result<Uuid, String> {
    let chain_proxy = normalize_chain_proxy(payload.chain_proxy.as_deref())?;
    let proxy_uuid = models::insert_proxy(
        &svc_ctx.db,
        &payload.name,
        &payload.host,
        payload.port,
        &payload.proxy_type,
        payload.username.as_deref(),
        payload.password.as_deref(),
        payload.country.as_deref(),
        payload.city.as_deref(),
        chain_proxy.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(proxy_uuid)
}

/// 获取代理列表
pub async fn get_proxies_service(
    svc_ctx: &SvcCtx,
    payload: &ListProxiesRequest,
) -> Result<(Vec<ProxyDto>, i64), String> {
    let filters = payload.filters.as_ref();
    let keyword = filters
        .and_then(|f| f.keyword.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let proxy_type = filters
        .and_then(|f| f.proxy_type.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let status = filters
        .and_then(|f| f.status.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let country = filters
        .and_then(|f| f.country.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let page = payload.pagination.page.max(1);
    let page_size = payload.pagination.page_size.max(1);
    let offset = (page - 1) * page_size;

    let proxies = models::fetch_proxies(
        &svc_ctx.db,
        keyword,
        proxy_type,
        status,
        country,
        offset,
        page_size,
    )
    .await
    .map_err(|e| e.to_string())?;

    let total = models::fetch_proxies_count(
        &svc_ctx.db,
        keyword,
        proxy_type,
        status,
        country,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok((proxies, total))
}

/// 获取代理详情
pub async fn get_proxy_service(svc_ctx: &SvcCtx, proxy_uuid: Uuid) -> Result<ProxyDto, String> {
    models::fetch_proxy_by_uuid(&svc_ctx.db, proxy_uuid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "代理不存在".to_string())
}

/// 更新代理
pub async fn update_proxy_service(
    svc_ctx: &SvcCtx,
    payload: &UpdateProxyRequest,
) -> Result<(), String> {
    // None：不修改；空字符串：清除；其他：校验后保存
    let chain_proxy = match payload.chain_proxy.as_deref() {
        None => None,
        Some(value) => Some(normalize_chain_proxy(Some(value))?.unwrap_or_default()),
    };
    models::update_proxy(
        &svc_ctx.db,
        payload.uuid,
        payload.name.as_deref(),
        payload.host.as_deref(),
        payload.port,
        payload.proxy_type.as_deref(),
        payload.username.as_deref(),
        payload.password.as_deref(),
        payload.country.as_deref(),
        payload.city.as_deref(),
        chain_proxy.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())
}

/// 批量设置（或清除）前置代理
pub async fn batch_set_chain_proxy_service(
    svc_ctx: &SvcCtx,
    payload: &BatchSetChainProxyRequest,
) -> Result<u64, String> {
    let chain_proxy = normalize_chain_proxy(payload.chain_proxy.as_deref())?;
    models::batch_set_proxy_chain(&svc_ctx.db, &payload.uuids, chain_proxy.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// 删除代理
pub async fn delete_proxy_service(svc_ctx: &SvcCtx, proxy_uuid: Uuid) -> Result<(), String> {
    // 先确认代理存在，再删除
    models::fetch_proxy_by_uuid(&svc_ctx.db, proxy_uuid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "代理不存在".to_string())?;

    models::delete_proxy(&svc_ctx.db, proxy_uuid).await.map_err(|e| e.to_string())
}

/// 批量删除代理
pub async fn batch_delete_proxies_service(
    svc_ctx: &SvcCtx,
    proxy_uuids: &[Uuid],
) -> Result<u64, String> {
    models::batch_delete_proxies(&svc_ctx.db, proxy_uuids)
        .await
        .map_err(|e| e.to_string())
}

/// 批量导入代理
///
/// 接收客户端已解析好的代理列表，直接保存到数据库
pub async fn batch_import_proxies_service(
    svc_ctx: &SvcCtx,
    payload: &BatchImportProxiesRequest,
) -> Result<crate::entitys::BatchImportResponse, String> {
    let mut success_count = 0;
    let mut failed_count = 0;
    let mut errors: Vec<String> = vec![];
    for (index, proxy) in payload.proxies.iter().enumerate() {
        let chain_proxy = match normalize_chain_proxy(proxy.chain_proxy.as_deref()) {
            Ok(chain_proxy) => chain_proxy,
            Err(e) => {
                failed_count += 1;
                errors.push(format!("第 {} 项: {}", index + 1, e));
                continue;
            }
        };
        let result = models::insert_proxy(
            &svc_ctx.db,
            &proxy.name,
            &proxy.host,
            proxy.port,
            &proxy.proxy_type,
            proxy.username.as_deref(),
            proxy.password.as_deref(),
            proxy.country.as_deref(),
            proxy.city.as_deref(),
            chain_proxy.as_deref(),
        )
        .await;

        match result {
            Ok(_) => {
                success_count += 1;
            }
            Err(e) => {
                failed_count += 1;
                errors.push(format!("第 {} 项: {}", index + 1, e));
            }
        }
    }

    Ok(crate::entitys::BatchImportResponse {
        success_count,
        failed_count,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_chain_proxy;

    #[test]
    fn chain_proxy_addresses_are_validated() {
        assert_eq!(normalize_chain_proxy(None).unwrap(), None);
        assert_eq!(normalize_chain_proxy(Some("  ")).unwrap(), None);
        assert_eq!(
            normalize_chain_proxy(Some(" http://127.0.0.1:7890 ")).unwrap().as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert!(normalize_chain_proxy(Some("socks5://u:p@[::1]:7891")).unwrap().is_some());
        assert!(normalize_chain_proxy(Some("127.0.0.1:7890")).unwrap().is_some());
        assert!(normalize_chain_proxy(Some("https://127.0.0.1:7890")).is_err());
        assert!(normalize_chain_proxy(Some("http://127.0.0.1")).is_err());
        assert!(normalize_chain_proxy(Some("http://:7890")).is_err());
    }
}
