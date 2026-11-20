//! 敏感字段落盘前用 Windows DPAPI 加密，绑定当前 Windows 用户。
//!
//! 保护的是数据文件被拷走、被其他用户读取的情况；同一用户下运行的程序仍然能解密。
//! 换电脑或换 Windows 用户后旧的密文解不开，读出来是空值，需要重新填写。
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use sqlx::Error;

use crate::database::{Db, Pool};

const PREFIX: &str = "dpapi:v1:";

#[cfg(windows)]
mod dpapi {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };
    use windows::core::PCWSTR;

    /// 额外熵：同一用户的其他程序用默认方式调 DPAPI 解不开这里的数据。
    const ENTROPY: &[u8] = b"MZBrowser.secret.v1";

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
    }

    /// 拷出系统分配的结果并释放
    unsafe fn take(output: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        unsafe {
            let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
            bytes
        }
    }

    pub fn encrypt(plain: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(plain);
        let entropy = blob(ENTROPY);
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            CryptProtectData(
                &input,
                PCWSTR::null(),
                Some(&entropy),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
            .map_err(|error| error.to_string())?;
            Ok(take(output))
        }
    }

    pub fn decrypt(cipher: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(cipher);
        let entropy = blob(ENTROPY);
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            CryptUnprotectData(
                &input,
                None,
                Some(&entropy),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
            .map_err(|error| error.to_string())?;
            Ok(take(output))
        }
    }
}

/// Linux/macOS（无桌面服务器）没有 DPAPI：用本机密钥文件做 AES-256-GCM。
/// 密钥文件默认 `$HOME/.config/mz-browser/secret.key`（`MZ_SECRET_KEY_FILE` 可改），权限 0600，
/// 第一次用到时生成。和 DPAPI 一样只防数据文件被单独拷走；密钥文件丢了旧密文就解不开。
/// 密文格式：12 字节 nonce + 密文 + 16 字节标签；前缀沿用 `dpapi:v1:`（含义是"本机保护"）。
#[cfg(not(windows))]
mod dpapi {
    use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
    use ring::rand::{SecureRandom, SystemRandom};
    use std::path::PathBuf;
    use std::sync::OnceLock;

    fn key_path() -> PathBuf {
        if let Some(path) = std::env::var_os("MZ_SECRET_KEY_FILE").filter(|p| !p.is_empty()) {
            return PathBuf::from(path);
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join(".config").join("mz-browser").join("secret.key")
    }

    fn load_or_create_key() -> Result<[u8; 32], String> {
        let path = key_path();
        if let Ok(bytes) = std::fs::read(&path) {
            return bytes
                .try_into()
                .map_err(|_| format!("secret key file {} is not 32 bytes", path.display()));
        }
        let mut key = [0u8; 32];
        SystemRandom::new().fill(&mut key).map_err(|_| "random key generation failed".to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("cannot create secret key file {}: {e}", path.display()))?;
        file.write_all(&key).map_err(|e| e.to_string())?;
        Ok(key)
    }

    fn key() -> Result<&'static LessSafeKey, String> {
        static KEY: OnceLock<Result<LessSafeKey, String>> = OnceLock::new();
        KEY.get_or_init(|| {
            let raw = load_or_create_key()?;
            UnboundKey::new(&AES_256_GCM, &raw)
                .map(LessSafeKey::new)
                .map_err(|_| "invalid AES key".to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
    }

    pub fn encrypt(plain: &[u8]) -> Result<Vec<u8>, String> {
        let key = key()?;
        let mut nonce = [0u8; NONCE_LEN];
        SystemRandom::new().fill(&mut nonce).map_err(|_| "random nonce failed".to_string())?;
        let mut data = plain.to_vec();
        key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut data)
            .map_err(|_| "encryption failed".to_string())?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&data);
        Ok(out)
    }

    pub fn decrypt(cipher: &[u8]) -> Result<Vec<u8>, String> {
        if cipher.len() < NONCE_LEN + 16 {
            return Err("ciphertext too short".into());
        }
        let key = key()?;
        let (nonce, rest) = cipher.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| "bad nonce".to_string())?;
        let mut data = rest.to_vec();
        let plain = key
            .open_in_place(nonce, Aad::empty(), &mut data)
            .map_err(|_| "decryption failed (wrong key?)".to_string())?;
        Ok(plain.to_vec())
    }
}

pub fn is_protected(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

/// 加密要写入磁盘的值。空值和已加密的值原样返回。
pub fn protect(plain: &str) -> String {
    if plain.is_empty() || is_protected(plain) {
        return plain.to_string();
    }
    match dpapi::encrypt(plain.as_bytes()) {
        Ok(cipher) => format!("{PREFIX}{}", BASE64.encode(cipher)),
        Err(error) => {
            tracing::warn!("could not encrypt a secret, storing it unprotected: {error}");
            plain.to_string()
        }
    }
}

/// 还原读出来的值。旧版本留下的明文原样返回；解不开（换了电脑或用户）时返回 None。
pub fn reveal(stored: &str) -> Option<String> {
    let Some(encoded) = stored.strip_prefix(PREFIX) else {
        return Some(stored.to_string());
    };
    let result = BASE64
        .decode(encoded)
        .map_err(|error| error.to_string())
        .and_then(|cipher| dpapi::decrypt(&cipher))
        .and_then(|plain| String::from_utf8(plain).map_err(|error| error.to_string()));
    match result {
        Ok(plain) => Some(plain),
        Err(error) => {
            tracing::warn!("could not decrypt a stored secret: {error}");
            None
        }
    }
}

/// 可为空的加密列。在 DTO 字段上用 `#[sqlx(try_from = "StoredSecret")]`，读出来就是明文。
#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub struct StoredSecret(Option<String>);

impl From<StoredSecret> for Option<String> {
    fn from(value: StoredSecret) -> Self {
        value.0.and_then(|stored| reveal(&stored))
    }
}

/// 非空的加密列。
#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub struct StoredText(String);

impl From<StoredText> for String {
    fn from(value: StoredText) -> Self {
        reveal(&value.0).unwrap_or_default()
    }
}

/// (表, 列)。表名和列名都是这里写死的常量，不来自外部输入。
const ENCRYPTED_COLUMNS: [(&str, &str); 5] = [
    ("proxies", "password"),
    ("platform_accounts", "password"),
    ("platform_accounts", "totp_secret"),
    ("environment_cookies", "value"),
    ("environment_passkeys", "private_key"),
];

/// 把旧版本留下的明文加密。可以重复执行，已加密的行不会再处理。
pub async fn encrypt_existing(pool: &Pool<Db>) -> Result<u64, Error> {
    let mut updated = 0;
    let mut tx = pool.begin().await?;
    for (table, column) in ENCRYPTED_COLUMNS {
        let select = format!(
            "SELECT id, {column} FROM {table} WHERE {column} IS NOT NULL AND {column} != '' AND {column} NOT LIKE '{PREFIX}%'"
        );
        let rows: Vec<(i64, String)> = sqlx::query_as(&select).fetch_all(&mut *tx).await?;
        let update = format!("UPDATE {table} SET {column} = $1 WHERE id = $2");
        for (id, plain) in rows {
            let protected = protect(&plain);
            if protected == plain {
                continue;
            }
            sqlx::query(&update).bind(protected).bind(id).execute(&mut *tx).await?;
            updated += 1;
        }
    }
    tx.commit().await?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_including_chinese() {
        let stored = protect("密码 p@ss");
        assert!(is_protected(&stored));
        assert!(!stored.contains("p@ss"));
        assert_eq!(reveal(&stored).as_deref(), Some("密码 p@ss"));
    }

    #[test]
    fn empty_and_already_protected_values_are_unchanged() {
        assert_eq!(protect(""), "");
        let stored = protect("x");
        assert_eq!(protect(&stored), stored);
    }

    #[test]
    fn legacy_plaintext_reads_as_is() {
        assert_eq!(reveal("old-password").as_deref(), Some("old-password"));
    }

    #[test]
    fn undecryptable_values_read_as_empty_rather_than_failing() {
        assert_eq!(reveal(&format!("{PREFIX}AAAA")), None);
        assert_eq!(reveal(&format!("{PREFIX}not base64!")), None);
        let column: Option<String> = StoredSecret(Some(format!("{PREFIX}AAAA"))).into();
        assert_eq!(column, None);
        let text: String = StoredText(format!("{PREFIX}AAAA")).into();
        assert_eq!(text, "");
    }
}
