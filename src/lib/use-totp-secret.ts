import { useTranslation } from 'react-i18next';
import { normalizeTotpSecret } from '@/lib/totp';

const TEXT = {
  zh: {
    label: '2FA 密钥',
    placeholder: '可选，粘贴 Base32 密钥或 otpauth:// 链接',
    preview: '当前验证码',
    invalidChar: '2FA 密钥只能包含字母 A-Z 和数字 2-7',
    tooShort: '2FA 密钥太短，至少 16 个字符',
    missingSecret: 'otpauth 链接里没有 secret 参数',
  },
  en: {
    label: '2FA secret',
    placeholder: 'Optional, paste a Base32 secret or an otpauth:// link',
    preview: 'Current code',
    invalidChar: 'The 2FA secret may only contain letters A-Z and digits 2-7',
    tooShort: 'The 2FA secret is too short (at least 16 characters)',
    missingSecret: 'The otpauth link has no secret parameter',
  },
};

export function useTotpText() {
  const { i18n } = useTranslation();
  return i18n.language?.toLowerCase().startsWith('zh') ? TEXT.zh : TEXT.en;
}

/** 校验 2FA 密钥，返回错误文案；合法或为空时返回空字符串 */
export function useTotpSecretError() {
  const text = useTotpText();
  return (value: string): string => {
    const result = normalizeTotpSecret(value);
    return result.ok ? '' : text[result.error];
  };
}
