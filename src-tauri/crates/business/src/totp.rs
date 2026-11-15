//! 账号两步验证（TOTP，RFC 6238）密钥的整理和校验。
//!
//! 验证码在前端和浏览器首页按当前时间计算，这里只负责把用户填的内容整理成标准 Base32 密钥。

/// 最短 16 个 Base32 字符（80 位），常见站点的密钥是 16 或 32 个字符。
const MIN_LEN: usize = 16;

/// 整理用户填写的 2FA 密钥。
///
/// - 支持纯 Base32 密钥（忽略大小写、空格、横线和末尾的 `=`）
/// - 支持 `otpauth://totp/...?secret=XXX` 链接，取其中的 secret
/// - 空字符串返回 `Ok(None)`，表示清除
pub fn normalize_secret(input: &str) -> Result<Option<String>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let raw = if trimmed.to_ascii_lowercase().starts_with("otpauth://") {
        secret_from_uri(trimmed).ok_or_else(|| "otpauth 链接里没有 secret 参数".to_string())?
    } else {
        trimmed.to_string()
    };
    let secret: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .trim_end_matches('=')
        .to_ascii_uppercase();
    if let Some(bad) = secret.chars().find(|c| !matches!(c, 'A'..='Z' | '2'..='7')) {
        return Err(format!("2FA 密钥只能包含字母 A-Z 和数字 2-7，发现非法字符「{bad}」"));
    }
    if secret.len() < MIN_LEN {
        return Err(format!("2FA 密钥太短，至少需要 {MIN_LEN} 个字符"));
    }
    Ok(Some(secret))
}

/// 从 otpauth 链接里取出 secret 参数（只做最基本的百分号解码）。
fn secret_from_uri(uri: &str) -> Option<String> {
    let query = uri.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        key.eq_ignore_ascii_case("secret").then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 给更新接口用：`None` 表示不修改，`Some("")` 表示清除，其它值整理后保存。
pub fn normalize_update(input: Option<&str>) -> Result<Option<String>, String> {
    match input {
        None => Ok(None),
        Some(value) => Ok(Some(normalize_secret(value)?.unwrap_or_default())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_secret_is_cleaned_up() {
        assert_eq!(
            normalize_secret(" aq57 lw7d-lgxk fjez e4we dplp vs2c z4zm== ").unwrap().as_deref(),
            Some("AQ57LW7DLGXKFJEZE4WEDPLPVS2CZ4ZM")
        );
    }

    #[test]
    fn otpauth_uri_is_accepted() {
        let uri = "otpauth://totp/Example:me%40example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example";
        assert_eq!(normalize_secret(uri).unwrap().as_deref(), Some("JBSWY3DPEHPK3PXP"));
        assert!(normalize_secret("otpauth://totp/x?issuer=a").is_err());
    }

    #[test]
    fn empty_means_cleared_and_bad_input_is_rejected() {
        assert_eq!(normalize_secret("  ").unwrap(), None);
        assert!(normalize_secret("JBSWY3DPEHPK3PX1").is_err());
        assert!(normalize_secret("JBSWY3DP").is_err());
        assert_eq!(normalize_update(None).unwrap(), None);
        assert_eq!(normalize_update(Some("")).unwrap().as_deref(), Some(""));
    }
}
