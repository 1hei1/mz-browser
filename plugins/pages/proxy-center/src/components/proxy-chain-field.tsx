import { useTranslation } from 'react-i18next';
import { Link2 } from 'lucide-react';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { DEFAULT_CHAIN_PROXY, isValidChainProxy } from '../utils/chain-proxy';

interface ProxyChainFieldProps {
  /** 前置代理地址；空字符串表示不使用链式代理 */
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}

/**
 * 链式代理（前置代理）设置。
 *
 * 浏览器 → 本机代理软件（前置代理）→ 当前代理 → 网站。
 * 适合本机直连不通、但经 Clash 等代理软件中转就能连上的 HTTP/SOCKS5 代理。
 */
export function ProxyChainField({ value, onChange, disabled }: ProxyChainFieldProps) {
  const { t } = useTranslation('proxy');
  const enabled = value.trim().length > 0;
  const invalid = enabled && !isValidChainProxy(value);

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-3">
        <Label
          htmlFor="proxy-chain-switch"
          className="flex items-center gap-2 text-xs font-medium text-muted-foreground"
        >
          <Link2 className="h-3.5 w-3.5" />
          {t('chain.label', { defaultValue: '链式代理（前置代理）' })}
        </Label>
        <Switch
          id="proxy-chain-switch"
          checked={enabled}
          disabled={disabled}
          onCheckedChange={(checked) => onChange(checked ? DEFAULT_CHAIN_PROXY : '')}
        />
      </div>
      {enabled && (
        <div className="space-y-1.5">
          <Input
            value={value}
            disabled={disabled}
            onChange={(event) => onChange(event.target.value)}
            onBlur={(event) => {
              // 输入框清空等于关闭链式代理
              if (!event.target.value.trim()) onChange('');
            }}
            className={`h-9 font-mono text-xs ${invalid ? 'border-destructive focus-visible:ring-destructive/40' : ''}`}
            placeholder={DEFAULT_CHAIN_PROXY}
          />
          <p
            className={`text-[10px] leading-relaxed ${invalid ? 'text-destructive' : 'text-muted-foreground'}`}
          >
            {invalid
              ? t('chain.invalid', {
                  defaultValue:
                    '格式应为 协议://主机:端口，例如 http://127.0.0.1:7890 或 socks5://127.0.0.1:7891',
                })
              : t('chain.hint', {
                  defaultValue:
                    '浏览器 → 前置代理（如本机 Clash 的 http://127.0.0.1:7890）→ 当前代理 → 网站。适合本机直连不通、经代理软件中转才能连上的代理。',
                })}
          </p>
        </div>
      )}
    </div>
  );
}
