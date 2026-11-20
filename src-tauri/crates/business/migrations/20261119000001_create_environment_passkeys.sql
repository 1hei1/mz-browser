-- 每环境独立通行密钥（passkey）：内核里的软件平台验证器创建的凭据，由内核经 EventBus 上报后落盘，
-- 下次启动随启动配置下发。私钥（PKCS#8 DER，base64url）和账号密码一样用 DPAPI 加密。
CREATE TABLE IF NOT EXISTS environment_passkeys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    environment_uuid TEXT NOT NULL,
    -- 凭据 ID（base64url，无填充）
    credential_id TEXT NOT NULL,
    -- 站点（RP ID），如 webauthn.io
    rp_id VARCHAR(255) NOT NULL,
    -- 用户句柄（base64url）
    user_handle TEXT NOT NULL DEFAULT '',
    user_name VARCHAR(255) NOT NULL DEFAULT '',
    user_display_name VARCHAR(255) NOT NULL DEFAULT '',
    private_key TEXT NOT NULL,
    sign_count INTEGER NOT NULL DEFAULT 0,
    discoverable INTEGER NOT NULL DEFAULT 1,
    -- 内核里的创建时间 / 最近一次使用时间，Unix 毫秒
    created_at_ms INTEGER NOT NULL DEFAULT 0,
    last_used_at_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_env_passkeys_env FOREIGN KEY (environment_uuid)
        REFERENCES environments(uuid) ON DELETE CASCADE,
    CONSTRAINT uk_env_passkeys_credential UNIQUE (environment_uuid, credential_id)
);

CREATE INDEX idx_env_passkeys_env_uuid ON environment_passkeys(environment_uuid);
