import { useEffect, useState } from 'react';
import { Copy, ShieldCheck } from 'lucide-react';
import { toast } from 'sonner';
import { TOTP_PERIOD, generateTotp, totpSecondsLeft } from '@/lib/totp';
import { cn } from '@/lib/utils';

interface TotpCodeProps {
  secret?: string | null;
  className?: string;
}

/** 实时显示 2FA 验证码，每秒刷新倒计时，点击复制 */
export function TotpCode({ secret, className }: TotpCodeProps) {
  // 记录验证码对应的密钥，密钥变化后旧验证码自动失效
  const [state, setState] = useState<{ secret: string; code: string }>({ secret: '', code: '' });
  const code = secret && state.secret === secret ? state.code : '';
  const [secondsLeft, setSecondsLeft] = useState<number>(totpSecondsLeft());

  useEffect(() => {
    if (!secret) return;
    let cancelled = false;
    let lastCounter = -1;
    const tick = () => {
      const now = Date.now();
      setSecondsLeft(totpSecondsLeft(now));
      const counter = Math.floor(now / 1000 / TOTP_PERIOD);
      if (counter === lastCounter) return;
      lastCounter = counter;
      generateTotp(secret, now)
        .then((value) => !cancelled && setState({ secret, code: value }))
        .catch(() => !cancelled && setState({ secret, code: '' }));
    };
    tick();
    const timer = window.setInterval(tick, 1000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [secret]);

  if (!secret) {
    return <span className={cn('text-sm text-muted-foreground', className)}>-</span>;
  }

  const urgent = secondsLeft <= 5;
  const handleCopy = async () => {
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code);
      toast.success('验证码已复制');
    } catch {
      toast.error('复制失败');
    }
  };

  return (
    <button
      type="button"
      onClick={() => void handleCopy()}
      title="点击复制验证码"
      className={cn(
        'inline-flex items-center gap-1.5 rounded px-1.5 py-0.5 hover:bg-accent transition-colors',
        className
      )}
    >
      <ShieldCheck className={cn('h-3.5 w-3.5', urgent ? 'text-destructive' : 'text-emerald-500')} />
      <span className={cn('font-mono text-sm tracking-wider', urgent && 'text-destructive')}>
        {code ? `${code.slice(0, 3)} ${code.slice(3)}` : '--- ---'}
      </span>
      <span className="text-[10px] tabular-nums text-muted-foreground w-5 text-right">
        {secondsLeft}s
      </span>
      <Copy className="h-3 w-3 text-muted-foreground" />
    </button>
  );
}
