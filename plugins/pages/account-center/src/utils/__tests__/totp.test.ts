import { describe, expect, it } from 'vitest';
import { generateTotp, normalizeTotpSecret, totpSecondsLeft } from '@/lib/totp';

// RFC 6238 附录 B 的 SHA1 测试密钥 "12345678901234567890" 的 Base32
const RFC_SECRET = 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ';

describe('totp', () => {
  it('matches RFC 6238 SHA1 test vectors (last 6 digits)', async () => {
    expect(await generateTotp(RFC_SECRET, 59_000)).toBe('287082');
    expect(await generateTotp(RFC_SECRET, 1111111109_000)).toBe('081804');
    expect(await generateTotp(RFC_SECRET, 1234567890_000)).toBe('005924');
    expect(await generateTotp(RFC_SECRET, 20000000000_000)).toBe('353130');
  });

  it('normalizes plain secrets and otpauth links', () => {
    expect(normalizeTotpSecret(' aq57 lw7d-lgxk fjez e4we dplp vs2c z4zm== ')).toEqual({
      ok: true,
      secret: 'AQ57LW7DLGXKFJEZE4WEDPLPVS2CZ4ZM',
    });
    expect(
      normalizeTotpSecret('otpauth://totp/Ex:me%40ex.com?secret=JBSWY3DPEHPK3PXP&issuer=Ex')
    ).toEqual({ ok: true, secret: 'JBSWY3DPEHPK3PXP' });
    expect(normalizeTotpSecret('')).toEqual({ ok: true, secret: '' });
    expect(normalizeTotpSecret('JBSWY3DPEHPK3PX1')).toEqual({ ok: false, error: 'invalidChar' });
    expect(normalizeTotpSecret('JBSWY3DP')).toEqual({ ok: false, error: 'tooShort' });
    expect(normalizeTotpSecret('otpauth://totp/x?issuer=a')).toEqual({
      ok: false,
      error: 'missingSecret',
    });
  });

  it('counts down within the 30s window', () => {
    expect(totpSecondsLeft(0)).toBe(30);
    expect(totpSecondsLeft(29_500)).toBe(1);
  });
});
