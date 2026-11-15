import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import {
  ChevronDown,
  Eye,
  EyeOff,
  Loader2,
  KeyRound,
  User,
} from 'lucide-react';
import { FormattedDialog, FormattedDialogFooter } from '@/components/formatted-dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { TextareaInput } from '@/components/textarea-input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
// @ts-ignore - Cross-plugin import
import { createAccount } from '../../../environment-manager/src/api';
import { TotpSecretField } from '@/components/totp-secret-field';
import { useTotpSecretError } from '@/lib/use-totp-secret';
import { PlatformIcon } from '@/components/platform-icon';

// 常见平台列表
const PLATFORMS = [
  { name: 'Facebook', url: 'https://www.facebook.com/' },
  { name: 'TikTok', url: 'https://www.tiktok.com/' },
  { name: 'YouTube', url: 'https://www.youtube.com/' },
  { name: 'X (Twitter)', url: 'https://x.com/' },
  { name: 'Instagram', url: 'https://www.instagram.com/' },
  { name: 'LinkedIn', url: 'https://www.linkedin.com/' },
  { name: 'Google', url: 'https://www.google.com/' },
  { name: 'Amazon', url: 'https://www.amazon.com/' },
];

// 根据平台 URL 获取图标组件
function getPlatformIcon(platform: string): React.ReactNode {
  return <PlatformIcon url={platform} />;
}

interface AccountFormData {
  platform: string;
  account: string;
  password: string;
  totpSecret: string;
}

interface CreateWindowCreateAccountDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onComplete?: () => void;
}

/**
 * 创建窗口创建账号对话框
 */
export function CreateWindowCreateAccountDialog({
  open,
  onOpenChange,
  onComplete,
}: CreateWindowCreateAccountDialogProps) {
  const { t } = useTranslation('create-window');

  // 表单数据
  const [formData, setFormData] = useState<AccountFormData>({
    platform: '',
    account: '',
    password: '',
    totpSecret: '',
  });
  const [showPassword, setShowPassword] = useState(false);
  const [platformError, setPlatformError] = useState('');
  const [totpError, setTotpError] = useState('');
  const totpSecretError = useTotpSecretError();
  const [submitting, setSubmitting] = useState(false);

  // 重置表单
  const resetForm = () => {
    setFormData({ platform: '', account: '', password: '', totpSecret: '' });
    setTotpError('');
    setShowPassword(false);
    setPlatformError('');
  };

  // 验证平台URL格式
  const validatePlatformUrl = (url: string): boolean => {
    if (!url.trim()) {
      setPlatformError(t('dialog.account.platformRequired') || '请输入平台 URL');
      return false;
    }
    try {
      const urlObj = new URL(url);
      if (!['http:', 'https:'].includes(urlObj.protocol)) {
        setPlatformError(
          t('dialog.account.invalidProtocol') || '平台 URL 必须以 http:// 或 https:// 开头'
        );
        return false;
      }
      setPlatformError('');
      return true;
    } catch {
      setPlatformError(t('dialog.account.invalidUrl') || '请输入正确的平台 URL');
      return false;
    }
  };

  const handlePlatformChange = (value: string) => {
    setFormData({ ...formData, platform: value });
    if (value) {
      validatePlatformUrl(value);
    } else {
      setPlatformError('');
    }
  };

  // 创建账号
  const handleCreate = async () => {
    if (!validatePlatformUrl(formData.platform)) {
      return;
    }
    if (!formData.account.trim()) {
      toast.warning(t('dialog.account.accountRequired') || '请输入账号');
      return;
    }
    const totpMessage = totpSecretError(formData.totpSecret);
    if (totpMessage) {
      setTotpError(totpMessage);
      return;
    }

    setSubmitting(true);
    try {
      await createAccount({
        platform_url: formData.platform,
        account: formData.account,
        password: formData.password || undefined,
        totp_secret: formData.totpSecret.trim() || undefined,
      });
      toast.success(t('dialog.account.createSuccess') || '账号创建成功');
      onOpenChange(false);
      resetForm();
      onComplete?.();
    } catch (e) {
      toast.error(
        e instanceof Error ? e.message : t('dialog.account.createFailed') || '创建账号失败'
      );
    } finally {
      setSubmitting(false);
    }
  };

  const handleClose = () => {
    resetForm();
    onOpenChange(false);
  };

  return (
    <FormattedDialog
      open={open}
      onOpenChange={onOpenChange}
      minWidth="min-w-[520px]"
      header={{
        icon: KeyRound,
        iconColor: 'text-blue-500',
        title: t('dialog.account.createTitle') || '创建账号',
        description: t('dialog.account.createDescription') || '创建一个新的平台账号',
        gradient: 'bg-gradient-to-r from-blue-500/10 via-indigo-500/10 to-blue-500/10',
        className: 'border-b border-border/50',
      }}
    >
      <div className="space-y-4">
        {/* 平台字段 */}
        <div className="space-y-1.5">
          <Label htmlFor="create-platform" className="text-xs font-medium text-foreground">
            {t('dialog.account.platform') || '平台'} <span className="text-destructive">*</span>
          </Label>
          <div className="relative flex items-center">
            <TextareaInput
              id="create-platform"
              value={formData.platform}
              onChange={(e) => handlePlatformChange(e.target.value)}
              onBlur={() => formData.platform && validatePlatformUrl(formData.platform)}
              className={`pr-8 text-sm min-h-9 ${platformError ? 'border-destructive focus-visible:border-destructive focus-visible:ring-destructive/50' : ''}`}
              placeholder={t('dialog.account.platformPlaceholder') || '请输入或选择平台 URL'}
            />
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type="button"
                  className="absolute right-0 top-0 h-9 w-9 flex items-center justify-center rounded-r"
                >
                  <ChevronDown className="h-4 w-4 text-muted-foreground" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-64">
                {PLATFORMS.map((platform) => (
                  <DropdownMenuItem
                    key={platform.url}
                    onClick={() => handlePlatformChange(platform.url)}
                    className="cursor-pointer"
                  >
                    <div className="flex items-center gap-2 w-full">
                      <span className="shrink-0">{getPlatformIcon(platform.url)}</span>
                      <span className="flex-1 text-xs truncate">{platform.url}</span>
                    </div>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          {platformError && <p className="text-[10px] text-destructive mt-0.5">{platformError}</p>}
        </div>

        {/* 账号字段 */}
        <div className="space-y-1.5">
          <Label htmlFor="create-account" className="text-xs font-medium text-foreground">
            {t('dialog.account.account') || '账号'} <span className="text-destructive">*</span>
          </Label>
          <div className="relative">
            <User className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <TextareaInput
              id="create-account"
              value={formData.account}
              onChange={(e) => setFormData({ ...formData, account: e.target.value })}
              className="pl-9 text-sm min-h-9"
              placeholder={t('dialog.account.accountPlaceholder') || '请输入账号'}
            />
          </div>
        </div>

        {/* 密码字段 */}
        <div className="space-y-1.5">
          <Label htmlFor="create-password" className="text-xs font-medium text-foreground">
            {t('dialog.account.password') || '密码'}
          </Label>
          <div className="relative">
            <TextareaInput
              id="create-password"
              value={formData.password}
              style={
                {
                  WebkitTextSecurity: showPassword ? 'none' : 'disc',
                  lineHeight: '1.75rem',
                } as React.CSSProperties
              }
              onChange={(e) => setFormData({ ...formData, password: e.target.value })}
              className="pr-9 text-sm min-h-9"
              placeholder={t('dialog.account.passwordPlaceholder') || '请输入密码（可选）'}
            />
            <button
              type="button"
              onClick={() => setShowPassword(!showPassword)}
              className="absolute right-0 top-0 h-9 w-9 flex items-center justify-center hover:bg-accent rounded-r transition-colors"
            >
              {showPassword ? (
                <EyeOff className="h-4 w-4 text-muted-foreground" />
              ) : (
                <Eye className="h-4 w-4 text-muted-foreground" />
              )}
            </button>
          </div>
        </div>

        {/* 2FA 密钥 */}
        <TotpSecretField
          id="cw-totpSecret"
          value={formData.totpSecret}
          error={totpError}
          onChange={(value) => {
            setFormData({ ...formData, totpSecret: value });
            setTotpError('');
          }}
        />
      </div>

      <FormattedDialogFooter>
        <Button variant="outline" size="sm" onClick={handleClose} disabled={submitting}>
          {t('dialog.account.cancel') || '取消'}
        </Button>
        <Button
          size="sm"
          className="bg-primary text-primary-foreground hover:bg-primary/90 border-0"
          onClick={handleCreate}
          disabled={submitting}
        >
          {submitting ? (
            <>
              <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />
              {t('dialog.account.creating') || '创建中...'}
            </>
          ) : (
            t('dialog.account.create') || '创建'
          )}
        </Button>
      </FormattedDialogFooter>
    </FormattedDialog>
  );
}
