import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link2, Loader2 } from 'lucide-react';
import { FormattedDialog, FormattedDialogFooter } from '@/components/formatted-dialog';
import { Button } from '@/components/ui/button';
import { ProxyChainField } from './proxy-chain-field';
import { DEFAULT_CHAIN_PROXY, isValidChainProxy } from '../utils/chain-proxy';

interface ProxyBatchChainDialogProps {
  open: boolean;
  count: number;
  onOpenChange: (open: boolean) => void;
  /** chainProxy 为空字符串表示清除选中代理的前置代理 */
  onConfirm: (chainProxy: string) => Promise<void>;
}

/**
 * 批量设置链式代理（前置代理）对话框
 */
export function ProxyBatchChainDialog({
  open,
  count,
  onOpenChange,
  onConfirm,
}: ProxyBatchChainDialogProps) {
  const { t } = useTranslation('proxy');
  const [chainProxy, setChainProxy] = useState(DEFAULT_CHAIN_PROXY);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (open) setChainProxy(DEFAULT_CHAIN_PROXY);
  }, [open]);

  const submit = async (value: string) => {
    setSubmitting(true);
    try {
      await onConfirm(value);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <FormattedDialog
      open={open}
      onOpenChange={onOpenChange}
      header={{
        icon: Link2,
        iconColor: 'text-blue-500',
        title: t('chain.batchTitle', { defaultValue: '设置链式代理' }),
        description: t('chain.batchDescription', {
          count,
          defaultValue: `为选中的 ${count} 个代理设置前置代理`,
        }),
        gradient: 'bg-gradient-to-r from-blue-500/10 via-indigo-500/10 to-blue-500/10',
      }}
      minWidth="max-w-[480px]"
    >
      <div className="rounded-lg border border-border/50 bg-muted/20 p-4">
        <ProxyChainField value={chainProxy} onChange={setChainProxy} disabled={submitting} />
      </div>
      <FormattedDialogFooter>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="text-xs mr-auto"
          disabled={submitting}
          onClick={() => void submit('')}
        >
          {t('chain.clear', { defaultValue: '清除前置代理' })}
        </Button>
        <Button
          variant="outline"
          size="sm"
          className="text-xs"
          disabled={submitting}
          onClick={() => onOpenChange(false)}
        >
          {t('dialog.edit.cancel')}
        </Button>
        <Button
          size="sm"
          className="text-xs"
          disabled={submitting || !chainProxy.trim() || !isValidChainProxy(chainProxy)}
          onClick={() => void submit(chainProxy)}
        >
          {submitting && <Loader2 className="h-3.5 w-3.5 mr-1.5 animate-spin" />}
          {t('dialog.edit.submit')}
        </Button>
      </FormattedDialogFooter>
    </FormattedDialog>
  );
}
