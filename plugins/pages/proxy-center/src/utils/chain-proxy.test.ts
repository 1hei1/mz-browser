import { describe, expect, it } from 'vitest';
import { isValidChainProxy } from './chain-proxy';

describe('isValidChainProxy', () => {
  it('accepts empty values and common front proxy addresses', () => {
    for (const value of [
      '',
      '   ',
      'http://127.0.0.1:7890',
      'socks5://127.0.0.1:7891',
      'socks5://user:pass@127.0.0.1:7891',
      '127.0.0.1:7890',
      'http://[::1]:7890/',
    ]) {
      expect(isValidChainProxy(value)).toBe(true);
    }
  });

  it('rejects unsupported schemes and missing or invalid ports', () => {
    for (const value of [
      'https://127.0.0.1:7890',
      'http://127.0.0.1',
      'http://127.0.0.1:0',
      'http://127.0.0.1:70000',
      'ftp://h:21',
    ]) {
      expect(isValidChainProxy(value)).toBe(false);
    }
  });
});
