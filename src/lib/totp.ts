/**
 * 两步验证（TOTP，RFC 6238）工具：整理密钥、计算 6 位验证码。
 * 与 Google Authenticator 默认参数一致：SHA1、6 位、30 秒。
 */

export const TOTP_PERIOD = 30;
const DIGITS = 6;
const MIN_SECRET_LENGTH = 16;
const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

export type TotpNormalizeResult =
  | { ok: true; secret: string }
  | { ok: false; error: 'invalidChar' | 'tooShort' | 'missingSecret' };

/**
 * 整理用户填写的密钥：支持纯 Base32（忽略大小写、空格、横线、末尾 =）和 otpauth:// 链接。
 * 空字符串返回 secret 为空。规则与后端 business::totp::normalize_secret 保持一致。
 */
export function normalizeTotpSecret(input: string): TotpNormalizeResult {
  const trimmed = input.trim();
  if (!trimmed) {
    return { ok: true, secret: '' };
  }
  let raw = trimmed;
  if (trimmed.toLowerCase().startsWith('otpauth://')) {
    const query = trimmed.split('?')[1]?.split('#')[0] ?? '';
    const param = query
      .split('&')
      .map((pair) => pair.split('='))
      .find(([key]) => key?.toLowerCase() === 'secret');
    if (!param || param[1] === undefined) {
      return { ok: false, error: 'missingSecret' };
    }
    try {
      raw = decodeURIComponent(param[1].replace(/\+/g, ' '));
    } catch {
      raw = param[1];
    }
  }
  const secret = raw.replace(/[\s-]/g, '').replace(/=+$/, '').toUpperCase();
  if (!/^[A-Z2-7]*$/.test(secret)) {
    return { ok: false, error: 'invalidChar' };
  }
  if (secret.length < MIN_SECRET_LENGTH) {
    return { ok: false, error: 'tooShort' };
  }
  return { ok: true, secret };
}

function base32Decode(secret: string): Uint8Array {
  let bits = 0;
  let value = 0;
  const out: number[] = [];
  for (const char of secret) {
    const index = BASE32_ALPHABET.indexOf(char);
    if (index < 0) continue;
    value = (value << 5) | index;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return new Uint8Array(out);
}

/** 计算指定时间（毫秒）的验证码 */
export async function generateTotp(secret: string, timeMs: number = Date.now()): Promise<string> {
  const keyBytes = base32Decode(secret.toUpperCase());
  const counter = Math.floor(timeMs / 1000 / TOTP_PERIOD);
  const message = new ArrayBuffer(8);
  const view = new DataView(message);
  view.setUint32(0, Math.floor(counter / 0x100000000));
  view.setUint32(4, counter >>> 0);
  const key = await crypto.subtle.importKey(
    'raw',
    keyBytes.slice().buffer as ArrayBuffer,
    { name: 'HMAC', hash: 'SHA-1' },
    false,
    ['sign']
  );
  const hmac = new Uint8Array(await crypto.subtle.sign('HMAC', key, message));
  const offset = hmac[hmac.length - 1] & 0x0f;
  const binary =
    ((hmac[offset] & 0x7f) << 24) |
    (hmac[offset + 1] << 16) |
    (hmac[offset + 2] << 8) |
    hmac[offset + 3];
  return String(binary % 10 ** DIGITS).padStart(DIGITS, '0');
}

/** 当前周期剩余秒数（1-30） */
export function totpSecondsLeft(timeMs: number = Date.now()): number {
  return TOTP_PERIOD - (Math.floor(timeMs / 1000) % TOTP_PERIOD);
}
