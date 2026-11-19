import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { listen } from '@tauri-apps/api/event';
import { AlertTriangle, KeyRound, Loader2, Trash2 } from 'lucide-react';
import { FormattedDialog, FormattedDialogFooter } from '@/components/formatted-dialog';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { useEnvironmentDialogStore, useRunningEnvsStore } from '../stores';
import {
  deleteEnvironmentPasskey,
  getEnvironmentPasskeys,
  type EnvironmentPasskeyItem,
} from '../api';

/** 内核新建 / 使用 / 删除通行密钥并由应用落盘后发出（不含私钥） */
export const ENVIRONMENT_PASSKEYS_CHANGED_EVENT = 'environment-passkeys-changed';

interface PasskeysChangedPayload {
  env_uuid: string;
  change: 'created' | 'updated' | 'deleted';
}

/** Unix 毫秒 → 本地时间；0 / 空 显示占位 */
function formatTime(ms: number | null | undefined, fallback: string): string {
  if (!ms || ms <= 0) return fallback;
  return new Date(ms).toLocaleString();
}

/**
 * 环境的通行密钥对话框：列出这个环境里网站创建的通行密钥（站点、用户名、创建时间），可删除。
 * 环境运行中新建或使用了通行密钥时，后端发 environment-passkeys-changed，这里自动刷新。
 */
export function EnvironmentPasskeysDialog() {
  const { t } = useTranslation('environment');
  const open = useEnvironmentDialogStore((state) => state.passkeysDialogOpen);
  const environment = useEnvironmentDialogStore((state) => state.passkeysEnvironment);
  const closeDialog = useEnvironmentDialogStore((state) => state.closePasskeysDialog);
  const isRunning = useRunningEnvsStore((state) =>
    environment ? state.isRunning(environment.uuid) : false
  );

  const [passkeys, setPasskeys] = useState<EnvironmentPasskeyItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<EnvironmentPasskeyItem | null>(null);
  const [deleting, setDeleting] = useState(false);

  const envUuid = environment?.uuid;

  // silent：事件触发的后台刷新不显示加载状态，避免列表闪烁
  const load = useCallback(
    async (silent = false) => {
      if (!envUuid) return;
      if (!silent) setLoading(true);
      try {
        setPasskeys(await getEnvironmentPasskeys(envUuid));
      } catch (e) {
        console.error('Failed to load passkeys:', e);
        if (!silent) {
          setPasskeys([]);
          toast.error(e instanceof Error ? e.message : t('dialog.passkeys.loadFailed'));
        }
      } finally {
        if (!silent) setLoading(false);
      }
    },
    [envUuid, t]
  );

  useEffect(() => {
    if (open && envUuid) {
      void load();
    }
  }, [open, envUuid, load]);

  // 对话框打开期间监听内核上报的变化，只刷新当前环境
  useEffect(() => {
    if (!open || !envUuid) return;
    const unlistenPromise = listen<PasskeysChangedPayload>(
      ENVIRONMENT_PASSKEYS_CHANGED_EVENT,
      (event) => {
        if (event.payload?.env_uuid === envUuid) {
          void load(true);
        }
      }
    );
    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [open, envUuid, load]);

  const handleClose = () => {
    setPasskeys([]);
    setPendingDelete(null);
    closeDialog();
  };

  const handleConfirmDelete = async () => {
    if (!pendingDelete) return;
    setDeleting(true);
    try {
      await deleteEnvironmentPasskey(pendingDelete.id);
      setPasskeys((prev) => prev.filter((item) => item.id !== pendingDelete.id));
      toast.success(t('dialog.passkeys.deleteSuccess'));
      setPendingDelete(null);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : t('dialog.passkeys.deleteFailed'));
    } finally {
      setDeleting(false);
    }
  };

  if (!environment) return null;

  return (
    <>
      <FormattedDialog
        open={open && !!environment}
        onOpenChange={(next) => {
          if (!next) handleClose();
        }}
        minWidth="min-w-[680px]"
        header={{
          icon: KeyRound,
          title: t('dialog.passkeys.title'),
          description: t('dialog.passkeys.description', { name: environment.name }),
        }}
      >
        <div className="space-y-2">
          {isRunning && passkeys.length > 0 ? (
            <p className="text-[10px] text-muted-foreground">{t('dialog.passkeys.runningHint')}</p>
          ) : null}

          {loading ? (
            <div className="flex items-center justify-center h-32 text-muted-foreground border border-border/50 rounded-md">
              <Loader2 className="h-5 w-5 animate-spin mr-2" />
              <span className="text-sm">{t('dialog.passkeys.loading')}</span>
            </div>
          ) : passkeys.length > 0 ? (
            <div className="border border-border/50 rounded-md">
              {/* 表头 */}
              <div className="grid grid-cols-[1.3fr_1.2fr_1fr_1fr_40px] gap-2 px-3 py-2 text-[11px] font-medium text-muted-foreground border-b border-border/50 bg-muted/30">
                <span>{t('dialog.passkeys.site')}</span>
                <span>{t('dialog.passkeys.user')}</span>
                <span>{t('dialog.passkeys.createdAt')}</span>
                <span>{t('dialog.passkeys.lastUsedAt')}</span>
                <span />
              </div>
              {/* 条目多时固定高度滚动（Radix ScrollArea 需要确定高度） */}
              <ScrollArea className={passkeys.length > 7 ? 'h-[300px]' : undefined}>
                <div className="divide-y divide-border/40">
                  {passkeys.map((item) => (
                    <div
                      key={item.id}
                      className="grid grid-cols-[1.3fr_1.2fr_1fr_1fr_40px] gap-2 items-center px-3 py-2 text-xs"
                    >
                      <span className="truncate font-medium" title={item.rp_id}>
                        {item.rp_id}
                      </span>
                      <span
                        className="truncate"
                        title={
                          item.user_display_name && item.user_display_name !== item.user_name
                            ? `${item.user_name} (${item.user_display_name})`
                            : item.user_name
                        }
                      >
                        {item.user_name || item.user_display_name || '-'}
                      </span>
                      <span className="truncate text-muted-foreground">
                        {formatTime(item.created_at_ms, '-')}
                      </span>
                      <span className="truncate text-muted-foreground">
                        {formatTime(item.last_used_at_ms, t('dialog.passkeys.neverUsed'))}
                      </span>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-muted-foreground hover:text-destructive"
                        title={t('dialog.passkeys.delete')}
                        onClick={() => setPendingDelete(item)}
                        disabled={deleting}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  ))}
                </div>
              </ScrollArea>
            </div>
          ) : (
            <div className="h-32 px-4 flex items-center justify-center text-center text-sm text-muted-foreground bg-muted/30 border border-border/50 rounded-md">
              {t('dialog.passkeys.empty')}
            </div>
          )}
        </div>

        <FormattedDialogFooter>
          <Button variant="outline" size="sm" onClick={handleClose}>
            {t('dialog.passkeys.close')}
          </Button>
        </FormattedDialogFooter>
      </FormattedDialog>

      {/* 删除确认 */}
      <AlertDialog
        open={!!pendingDelete}
        onOpenChange={(next) => {
          if (!next && !deleting) setPendingDelete(null);
        }}
      >
        <AlertDialogContent className="gap-0 p-3">
          <AlertDialogHeader className="mb-3">
            <AlertDialogTitle className="text-sm font-semibold mb-0 flex items-center gap-1.5">
              <AlertTriangle className="h-4 w-4 text-destructive" />
              {t('dialog.passkeys.deleteTitle')}
            </AlertDialogTitle>
            <AlertDialogDescription className="text-xs text-muted-foreground mt-1 mb-0">
              {pendingDelete
                ? t('dialog.passkeys.deleteDescription', {
                    site: pendingDelete.rp_id,
                    user: pendingDelete.user_name || pendingDelete.user_display_name || '-',
                  })
                : null}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="mt-4 gap-2">
            <AlertDialogCancel className="text-xs px-3 py-1.5 h-7" disabled={deleting}>
              {t('dialog.passkeys.cancel')}
            </AlertDialogCancel>
            {/* 用普通按钮：删除完成前不自动关闭确认框 */}
            <Button
              size="sm"
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90 text-xs px-3 py-1.5 h-7"
              onClick={() => void handleConfirmDelete()}
              disabled={deleting}
            >
              {deleting ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" />
                  {t('dialog.passkeys.deleting')}
                </>
              ) : (
                t('dialog.passkeys.confirmDelete')
              )}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
