import { Label } from '@/components/ui/label';
import { TextareaInput } from '@/components/textarea-input';
import { TotpCode } from '@/components/totp-code';
import { normalizeTotpSecret } from '@/lib/totp';
import { useTotpText } from '@/lib/use-totp-secret';

interface TotpSecretFieldProps {
  id: string;
  value: string;
  error?: string;
  onChange: (value: string) => void;
  labelClassName?: string;
  inputClassName?: string;
}

/** 账号表单里的 2FA 密钥输入框，填对后实时预览验证码 */
export function TotpSecretField({
  id,
  value,
  error,
  onChange,
  labelClassName = 'text-xs text-muted-foreground',
  inputClassName = 'text-sm min-h-9',
}: TotpSecretFieldProps) {
  const text = useTotpText();
  const normalized = normalizeTotpSecret(value);
  const preview = normalized.ok && normalized.secret ? normalized.secret : '';

  return (
    <div className="space-y-1">
      <Label htmlFor={id} className={labelClassName}>
        {text.label}
      </Label>
      <TextareaInput
        id={id}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        spellCheck={false}
        autoComplete="off"
        className={`font-mono ${inputClassName} ${error ? 'border-destructive focus-visible:border-destructive focus-visible:ring-destructive/50' : ''}`}
        placeholder={text.placeholder}
      />
      {error ? (
        <p className="text-[10px] text-destructive mt-0.5">{error}</p>
      ) : (
        preview && (
          <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
            {text.preview}
            <TotpCode secret={preview} />
          </div>
        )
      )}
    </div>
  );
}
