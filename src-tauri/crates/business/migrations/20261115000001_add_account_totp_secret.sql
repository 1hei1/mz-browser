-- 账号两步验证（2FA / TOTP）密钥，Base32 格式，和密码一样用 DPAPI 加密落盘。为空表示没有设置。
ALTER TABLE platform_accounts ADD COLUMN totp_secret TEXT;
