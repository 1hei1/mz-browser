//! 每环境独立通行密钥（passkey）。
//!
//! 内核里每个环境有一个软件平台验证器（替代 Windows Hello），站点创建的通行密钥只属于这个环境。
//! 内核在创建、签名计数变化、删除时经 EventBus 上报（见 [`apply_kernel_event`]），这里加密落盘；
//! 启动环境时 [`launch_passkeys`] 读出来随启动配置下发给内核。私钥用 DPAPI 加密（`secret::protect`）。
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::svc_ctx::SvcCtx;

/// 列表展示用，不含私钥
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentPasskeyDto {
    pub id: i64,
    pub environment_uuid: Uuid,
    pub credential_id: String,
    pub rp_id: String,
    pub user_name: String,
    pub user_display_name: String,
    pub sign_count: i64,
    pub discoverable: bool,
    pub created_at_ms: i64,
    pub last_used_at_ms: Option<i64>,
}

/// 完整凭据（含私钥明文），只用于启动下发和导出。字段名和内核 LaunchConfig.passkeys 一致
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasskeyCredential {
    /// base64url（无填充）
    pub credential_id: String,
    pub rp_id: String,
    /// base64url
    #[serde(default)]
    pub user_handle: String,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub user_display_name: String,
    /// PKCS#8 DER 私钥，base64url
    pub private_key: String,
    #[serde(default)]
    pub sign_count: u32,
    /// Unix 毫秒
    #[serde(default)]
    pub created_at: i64,
    #[serde(default = "default_true")]
    pub discoverable: bool,
}

fn default_true() -> bool {
    true
}

#[derive(FromRow)]
struct PasskeyRow {
    credential_id: String,
    rp_id: String,
    user_handle: String,
    user_name: String,
    user_display_name: String,
    #[sqlx(try_from = "crate::secret::StoredText")]
    private_key: String,
    sign_count: i64,
    discoverable: bool,
    created_at_ms: i64,
}

impl From<PasskeyRow> for PasskeyCredential {
    fn from(row: PasskeyRow) -> Self {
        Self {
            credential_id: row.credential_id,
            rp_id: row.rp_id,
            user_handle: row.user_handle,
            user_name: row.user_name,
            user_display_name: row.user_display_name,
            private_key: row.private_key,
            sign_count: row.sign_count.clamp(0, u32::MAX as i64) as u32,
            created_at: row.created_at_ms,
            discoverable: row.discoverable,
        }
    }
}

/// 内核上报的凭据变化（EventBus Topic 0x0901 的 JSON）。数字字段内核按 double 写出
#[derive(Debug, Clone, Deserialize)]
pub struct PasskeyKernelEvent {
    pub action: String,
    pub credential_id: String,
    #[serde(default)]
    pub rp_id: Option<String>,
    #[serde(default)]
    pub user_handle: Option<String>,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub user_display_name: Option<String>,
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub sign_count: Option<f64>,
    #[serde(default)]
    pub created_at: Option<f64>,
    #[serde(default)]
    pub discoverable: Option<bool>,
}

/// 处理结果（给上层发界面刷新事件、写日志用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PasskeyChange {
    Created,
    Updated,
    Deleted,
    Ignored,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn number(value: Option<f64>) -> i64 {
    value.filter(|value| value.is_finite() && *value >= 0.0).map(|value| value as i64).unwrap_or(0)
}

/// 环境的通行密钥列表（不含私钥），按站点、用户名排序
pub async fn list_environment_passkeys_service(
    svc_ctx: &SvcCtx,
    env_uuid: Uuid,
) -> Result<Vec<EnvironmentPasskeyDto>, String> {
    sqlx::query_as::<_, EnvironmentPasskeyDto>(
        r#"
        SELECT id, environment_uuid, credential_id, rp_id, user_name, user_display_name,
               sign_count, discoverable, created_at_ms, last_used_at_ms
        FROM environment_passkeys
        WHERE environment_uuid = $1
        ORDER BY rp_id, user_name, id
        "#,
    )
    .bind(env_uuid)
    .fetch_all(&svc_ctx.db)
    .await
    .map_err(|e| e.to_string())
}

/// 删除一条通行密钥。环境运行中删除，要等环境重启后内核里才会消失
pub async fn delete_environment_passkey_service(svc_ctx: &SvcCtx, id: i64) -> Result<(), String> {
    sqlx::query("DELETE FROM environment_passkeys WHERE id = $1")
        .bind(id)
        .execute(&svc_ctx.db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 启动下发用：环境的全部凭据（含私钥明文）。私钥解不开（换了电脑或 Windows 用户）的跳过
pub async fn launch_passkeys(svc_ctx: &SvcCtx, env_uuid: Uuid) -> Result<Vec<PasskeyCredential>, String> {
    let rows = sqlx::query_as::<_, PasskeyRow>(
        r#"
        SELECT credential_id, rp_id, user_handle, user_name, user_display_name, private_key,
               sign_count, discoverable, created_at_ms
        FROM environment_passkeys
        WHERE environment_uuid = $1
        ORDER BY id
        "#,
    )
    .bind(env_uuid)
    .fetch_all(&svc_ctx.db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(PasskeyCredential::from)
        .filter(|credential| !credential.private_key.is_empty())
        .collect())
}

/// 写入或覆盖一条完整凭据（内核新建、导入环境）
pub async fn upsert_passkey(
    svc_ctx: &SvcCtx,
    env_uuid: Uuid,
    credential: &PasskeyCredential,
) -> Result<(), String> {
    if credential.credential_id.is_empty() || credential.rp_id.is_empty() || credential.private_key.is_empty() {
        return Err("通行密钥数据不完整".to_string());
    }
    let created_at = if credential.created_at > 0 { credential.created_at } else { now_ms() };
    sqlx::query(
        r#"
        INSERT INTO environment_passkeys
            (environment_uuid, credential_id, rp_id, user_handle, user_name, user_display_name,
             private_key, sign_count, discoverable, created_at_ms)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (environment_uuid, credential_id) DO UPDATE SET
            rp_id = excluded.rp_id,
            user_handle = excluded.user_handle,
            user_name = excluded.user_name,
            user_display_name = excluded.user_display_name,
            private_key = excluded.private_key,
            sign_count = excluded.sign_count,
            discoverable = excluded.discoverable,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(env_uuid)
    .bind(&credential.credential_id)
    .bind(&credential.rp_id)
    .bind(&credential.user_handle)
    .bind(&credential.user_name)
    .bind(&credential.user_display_name)
    .bind(crate::secret::protect(&credential.private_key))
    .bind(credential.sign_count as i64)
    .bind(credential.discoverable)
    .bind(created_at)
    .execute(&svc_ctx.db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 处理内核上报的一条变化。环境已被删除（外键不存在）时忽略
pub async fn apply_kernel_event(
    svc_ctx: &SvcCtx,
    env_uuid: Uuid,
    event: &PasskeyKernelEvent,
) -> Result<PasskeyChange, String> {
    if event.credential_id.is_empty() {
        return Ok(PasskeyChange::Ignored);
    }
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM environments WHERE uuid = $1")
        .bind(env_uuid)
        .fetch_one(&svc_ctx.db)
        .await
        .map_err(|e| e.to_string())?;
    if exists == 0 {
        return Ok(PasskeyChange::Ignored);
    }
    match event.action.as_str() {
        "deleted" => {
            sqlx::query("DELETE FROM environment_passkeys WHERE environment_uuid = $1 AND credential_id = $2")
                .bind(env_uuid)
                .bind(&event.credential_id)
                .execute(&svc_ctx.db)
                .await
                .map_err(|e| e.to_string())?;
            Ok(PasskeyChange::Deleted)
        }
        "created" | "updated" => {
            let credential = PasskeyCredential {
                credential_id: event.credential_id.clone(),
                rp_id: event.rp_id.clone().unwrap_or_default(),
                user_handle: event.user_handle.clone().unwrap_or_default(),
                user_name: event.user_name.clone().unwrap_or_default(),
                user_display_name: event.user_display_name.clone().unwrap_or_default(),
                private_key: event.private_key.clone().unwrap_or_default(),
                sign_count: number(event.sign_count).clamp(0, u32::MAX as i64) as u32,
                created_at: number(event.created_at),
                discoverable: event.discoverable.unwrap_or(true),
            };
            upsert_passkey(svc_ctx, env_uuid, &credential).await?;
            if event.action == "updated" {
                // 计数变化 = 刚用它登录过
                sqlx::query(
                    "UPDATE environment_passkeys SET last_used_at_ms = $1 WHERE environment_uuid = $2 AND credential_id = $3",
                )
                .bind(now_ms())
                .bind(env_uuid)
                .bind(&event.credential_id)
                .execute(&svc_ctx.db)
                .await
                .map_err(|e| e.to_string())?;
                Ok(PasskeyChange::Updated)
            } else {
                Ok(PasskeyChange::Created)
            }
        }
        _ => Ok(PasskeyChange::Ignored),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn context_with_environment() -> (SvcCtx, Uuid) {
        let mut config = crate::utils::DatabaseConfig::embedded("sqlite::memory:");
        config.max_connections = 1;
        config.min_connections = 1;
        let db = crate::database::connect(&config).await.expect("connect");
        crate::database::migrate(&db).await.expect("migrate");
        let svc_ctx = SvcCtx { db };
        let uuid = Uuid::new_v4();
        sqlx::query("INSERT INTO environments (uuid, name) VALUES ($1, 'passkey test')")
            .bind(uuid)
            .execute(&svc_ctx.db)
            .await
            .expect("insert environment");
        (svc_ctx, uuid)
    }

    fn event(action: &str, sign_count: f64) -> PasskeyKernelEvent {
        PasskeyKernelEvent {
            action: action.to_string(),
            credential_id: "Y3JlZA".to_string(),
            rp_id: Some("webauthn.io".to_string()),
            user_handle: Some("dXNlcg".to_string()),
            user_name: Some("alice".to_string()),
            user_display_name: Some("Alice".to_string()),
            private_key: Some("cHJpdmF0ZQ".to_string()),
            sign_count: Some(sign_count),
            created_at: Some(1_763_539_200_000.0),
            discoverable: Some(true),
        }
    }

    #[tokio::test]
    async fn kernel_events_create_update_and_delete_passkeys() {
        let (svc_ctx, env) = context_with_environment().await;

        let change = apply_kernel_event(&svc_ctx, env, &event("created", 1.0)).await.unwrap();
        assert_eq!(change, PasskeyChange::Created);
        let launch = launch_passkeys(&svc_ctx, env).await.unwrap();
        assert_eq!(launch.len(), 1);
        assert_eq!(launch[0].private_key, "cHJpdmF0ZQ");
        assert_eq!(launch[0].sign_count, 1);
        assert_eq!(launch[0].created_at, 1_763_539_200_000);

        let change = apply_kernel_event(&svc_ctx, env, &event("updated", 5.0)).await.unwrap();
        assert_eq!(change, PasskeyChange::Updated);
        let listed = list_environment_passkeys_service(&svc_ctx, env).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].sign_count, 5);
        assert_eq!(listed[0].rp_id, "webauthn.io");
        assert_eq!(listed[0].user_name, "alice");
        assert!(listed[0].last_used_at_ms.is_some());
        // 创建时间不被更新覆盖
        assert_eq!(listed[0].created_at_ms, 1_763_539_200_000);

        let deleted = PasskeyKernelEvent { action: "deleted".to_string(), ..event("deleted", 0.0) };
        assert_eq!(apply_kernel_event(&svc_ctx, env, &deleted).await.unwrap(), PasskeyChange::Deleted);
        assert!(launch_passkeys(&svc_ctx, env).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn events_for_unknown_environments_are_ignored() {
        let (svc_ctx, _env) = context_with_environment().await;
        let change = apply_kernel_event(&svc_ctx, Uuid::new_v4(), &event("created", 1.0)).await.unwrap();
        assert_eq!(change, PasskeyChange::Ignored);
    }

    #[tokio::test]
    async fn deleting_by_id_and_deleting_the_environment_remove_passkeys() {
        let (svc_ctx, env) = context_with_environment().await;
        apply_kernel_event(&svc_ctx, env, &event("created", 1.0)).await.unwrap();
        let id = list_environment_passkeys_service(&svc_ctx, env).await.unwrap()[0].id;
        delete_environment_passkey_service(&svc_ctx, id).await.unwrap();
        assert!(list_environment_passkeys_service(&svc_ctx, env).await.unwrap().is_empty());

        apply_kernel_event(&svc_ctx, env, &event("created", 1.0)).await.unwrap();
        crate::models::permanent_delete_environment(&svc_ctx.db, env).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM environment_passkeys")
            .fetch_one(&svc_ctx.db)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }
}
