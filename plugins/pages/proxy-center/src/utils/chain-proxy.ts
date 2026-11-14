/**
 * 链式代理（前置代理）地址工具
 */

/** 打开开关时预填的前置代理地址（Clash / Clash Verge 默认的混合端口） */
export const DEFAULT_CHAIN_PROXY = 'http://127.0.0.1:7890';

const CHAIN_PROXY_PATTERN =
  /^(?:(?:http|socks5h?|socks):\/\/)?(?:[^@/\s]+@)?(?:\[[^\]\s]+\]|[^:/\s@]+):(\d{1,5})\/?$/i;

/** 前置代理地址格式是否有效（空字符串视为“未设置”，也算有效） */
export function isValidChainProxy(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return true;
  const match = trimmed.match(CHAIN_PROXY_PATTERN);
  if (!match) return false;
  const port = Number(match[1]);
  return port > 0 && port <= 65535;
}
