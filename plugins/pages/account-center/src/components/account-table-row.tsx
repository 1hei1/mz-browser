import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  MoreVertical,
  Copy,
  Edit,
  Trash2,
  Eye,
  EyeOff,
  Monitor,
  Loader2,
} from 'lucide-react';
import { toast } from 'sonner';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu';
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip';
import {
  DataTableRowContainer,
  DataTableCell,
  DataTableCheckboxCell,
  DataTableActionsCell,
} from '@/components/data-table';
import { TotpCode } from '@/components/totp-code';
import type { Account } from '../types';
import { decryptAccountPassword } from '../utils/password';
import { PlatformIcon } from '@/components/platform-icon';

interface AccountTableRowProps {
  account: Account;
  isSelected: boolean;
  onSelect: (uuid: string, selected: boolean) => void;
  onEdit: (account: Account) => void;
  onDelete: (account: Account) => void;
}

// 获取平台图标
function getPlatformIcon(platform: string): React.ReactNode {
  return <PlatformIcon url={platform} />;
}

// 状态徽章
function StatusBadge({ status }: { status: Account['status'] }) {
  const { t } = useTranslation('account');
  const statusConfig = {
    active: {
      className: 'bg-emerald-500/10 text-emerald-500',
      dot: 'bg-emerald-500',
    },
    inactive: {
      className: 'bg-slate-500/10 text-slate-500',
      dot: 'bg-slate-500',
    },
    expired: {
      className: 'bg-destructive/10 text-destructive',
      dot: 'bg-destructive',
    },
  };

  const config = statusConfig[status];

  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium ${config.className}`}
    >
      <span className={`w-1.5 h-1.5 rounded-full ${config.dot}`}></span>
      {t(`status.${status}`)}
    </span>
  );
}

export function AccountTableRow({
  account,
  isSelected,
  onSelect,
  onEdit,
  onDelete,
}: AccountTableRowProps) {
  const { t } = useTranslation('account');
  const [showPassword, setShowPassword] = useState(false);
  const [decryptedPassword, setDecryptedPassword] = useState<string | null>(null);
  const [decryptingPassword, setDecryptingPassword] = useState(false);

  const handleCopy = async (text: string, successText: string) => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      toast.success(successText);
    } catch (error) {
      console.error('Copy failed:', error);
      toast.error(t('toast.copyFailed'));
    }
  };

  const ensurePlainPassword = async () => {
    if (decryptedPassword !== null) {
      return decryptedPassword;
    }

    setDecryptingPassword(true);
    try {
      const plainPassword = await decryptAccountPassword(account);
      setDecryptedPassword(plainPassword);
      return plainPassword;
    } catch (error) {
      console.error('Decrypt password failed:', error);
      toast.error('密码解密失败');
      return null;
    } finally {
      setDecryptingPassword(false);
    }
  };

  const handleTogglePassword = async () => {
    if (showPassword) {
      setShowPassword(false);
      return;
    }

    const plainPassword = await ensurePlainPassword();
    if (plainPassword !== null) {
      setShowPassword(true);
    }
  };

  const handleCopyPassword = async () => {
    const plainPassword = await ensurePlainPassword();
    if (plainPassword !== null) {
      await handleCopy(plainPassword, t('toast.passwordCopied'));
    }
  };

  const handleRowMouseLeave = () => {
    setShowPassword(false);
  };

  const formatDate = (dateString?: string) => {
    if (!dateString || dateString === '-') return '-';
    try {
      const date = new Date(dateString);
      return date.toLocaleDateString('zh-CN', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
      });
    } catch {
      return dateString;
    }
  };

  return (
    <DataTableRowContainer isSelected={isSelected} onMouseLeave={handleRowMouseLeave}>
      {/* 选择框列 */}
      <DataTableCheckboxCell
        isSelected={isSelected}
        onSelect={(selected) => onSelect(account.uuid, selected)}
      />

      {/* 平台列 */}
      <DataTableCell>
        <div className="flex items-center gap-2">
          {getPlatformIcon(account.platform)}
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="text-sm font-medium cursor-help">{account.platformName}</span>
            </TooltipTrigger>
            <TooltipContent>{account.platform}</TooltipContent>
          </Tooltip>
        </div>
      </DataTableCell>

      {/* 账号列 */}
      <DataTableCell>
        <div className="flex items-center gap-1.5">
          <span className="font-mono text-sm">{account.account}</span>
          <button
            onClick={() => void handleCopy(account.account, t('toast.accountCopied'))}
            className="opacity-0 group-hover:opacity-100 transition-opacity p-1 hover:bg-accent rounded"
            title={t('table.actions.copy')}
          >
            <Copy className="h-3 w-3 text-muted-foreground" />
          </button>
        </div>
      </DataTableCell>

      {/* 密码列 */}
      <DataTableCell>
        <div className="flex items-center gap-1.5">
          <span className="font-mono text-sm">
            {showPassword ? decryptedPassword || '' : '••••••••'}
          </span>
          <button
            onClick={() => void handleTogglePassword()}
            disabled={decryptingPassword}
            className="p-1 hover:bg-accent rounded transition-colors"
          >
            {decryptingPassword ? (
              <Loader2 className="h-3 w-3 text-muted-foreground animate-spin" />
            ) : showPassword ? (
              <EyeOff className="h-3 w-3 text-muted-foreground" />
            ) : (
              <Eye className="h-3 w-3 text-muted-foreground" />
            )}
          </button>
          <button
            onClick={() => void handleCopyPassword()}
            disabled={decryptingPassword}
            className="opacity-0 group-hover:opacity-100 transition-opacity p-1 hover:bg-accent rounded"
            title={t('table.actions.copyPassword')}
          >
            <Copy className="h-3 w-3 text-muted-foreground" />
          </button>
        </div>
      </DataTableCell>

      {/* 2FA 验证码列 */}
      <DataTableCell>
        <TotpCode secret={account.totpSecret} />
      </DataTableCell>

      {/* 状态列 */}
      <DataTableCell>
        <StatusBadge status={account.status} />
      </DataTableCell>

      {/* 关联环境列 */}
      <DataTableCell>
        <div className="flex items-center gap-1.5">
          <Monitor className="h-3 w-3 text-muted-foreground" />
          <span className="font-mono text-sm text-foreground">{account.environmentsCount}</span>
        </div>
      </DataTableCell>

      {/* 最后使用列 */}
      <DataTableCell>
        <div className="text-sm text-muted-foreground">{formatDate(account.lastUsedAt)}</div>
      </DataTableCell>

      {/* 创建时间列 */}
      <DataTableCell>
        <div className="text-sm text-muted-foreground">{formatDate(account.createdAt)}</div>
      </DataTableCell>

      {/* 操作列 */}
      <DataTableActionsCell isSelected={isSelected}>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              className="h-7 w-7 flex items-center justify-center rounded hover:bg-accent text-muted-foreground hover:text-foreground"
              title={t('table.actions.more')}
            >
              <MoreVertical className="h-4 w-4" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-40">
            <DropdownMenuItem onClick={() => onEdit(account)} className="cursor-pointer">
              <Edit className="w-4 h-4 mr-2" />
              <span>{t('table.actions.edit')}</span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onClick={() => onDelete(account)}
              className="cursor-pointer text-destructive focus:text-destructive"
            >
              <Trash2 className="w-4 h-4 mr-2" />
              <span>{t('table.actions.delete')}</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </DataTableActionsCell>
    </DataTableRowContainer>
  );
}
