import { Fragment, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  isTauri,
  type AuditRecord,
  type VaultEntryView,
  type VaultStatus,
} from "../api";
import { useAppStore } from "../store";
import { clockLabel, dayLabel, groupByDay, parseStamp } from "../timeline";
import { KeyManagerDialog } from "./KeyManagerDialog";
import { KnownHostsDialog } from "./KnownHostsDialog";
import { SelectSetting } from "./SettingsControls";
import { DaySeparator, Skeleton } from "./ui";
import { UnlockDialog, type VaultAction } from "./UnlockDialog";

const AUTO_LOCK_CHOICES = [0, 5, 15, 30, 60];

export function CredentialVault() {
  const { t } = useTranslation();
  const autoLockMinutes = useAppStore((s) => s.autoLockMinutes);
  const setAutoLockMinutes = useAppStore((s) => s.setAutoLockMinutes);
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [entries, setEntries] = useState<VaultEntryView[]>([]);
  const [audit, setAudit] = useState<AuditRecord[]>([]);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [action, setAction] = useState<VaultAction | null>(null);
  const [keyManagerOpen, setKeyManagerOpen] = useState(false);
  const [knownHostsOpen, setKnownHostsOpen] = useState(false);
  const [confirmingClear, setConfirmingClear] = useState(false);

  const refresh = useCallback(async () => {
    if (!isTauri) return;
    try {
      const [nextStatus, nextEntries, nextAudit] = await Promise.all([
        api.vaultStatus(),
        api.vaultListEntries(),
        api.credentialAuditList(20),
      ]);
      setStatus(nextStatus);
      setEntries(nextEntries);
      setAudit(nextAudit);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const finish = (detail?: string) => {
    setMessage(detail ?? t("vault.updated"));
    setError("");
    setConfirmingClear(false);
    void refresh();
  };

  const lock = async () => {
    try {
      await api.vaultLock();
      finish(t("vault.locked"));
    } catch (err) {
      setError(String(err));
    }
  };

  const deleteEntry = async (entry: VaultEntryView) => {
    try {
      await api.vaultDeleteEntry(entry.secretRef);
      finish();
    } catch (err) {
      setError(String(err));
    }
  };

  const clearEntries = async () => {
    try {
      const count = await api.vaultClearEntries();
      finish(t("vault.cleared", { count }));
    } catch (err) {
      setError(String(err));
    }
  };

  const masterMode = status?.mode === "master";

  // First paint: hold the pane's shape instead of showing "local vault" before
  // the real mode is known.
  if (isTauri && status === null && !error) {
    return <Skeleton rows={4} label={t("common.loading")} />;
  }

  return (
    <>
      <fieldset className="settings-group vault-overview">
        <legend>{t("vault.overviewTitle")}</legend>
        {!isTauri && <p className="hint">{t("vault.desktopOnly")}</p>}
        <div className="vault-status-line">
          <span
            className={
              masterMode
                ? status?.locked
                  ? "badge warn"
                  : "badge ok"
                : "badge ok"
            }
          >
            {masterMode
              ? status?.locked
                ? t("vault.stateLocked")
                : t("vault.stateUnlocked")
              : t("vault.stateLocal")}
          </span>
          <span className="hint">
            {t(masterMode ? "vault.modeMaster" : "vault.modeLocal")}
          </span>
        </div>
        <p className="hint">{t("vault.entriesCount", { count: status?.entries ?? 0 })}</p>
        <div className="plugin-actions">
          {masterMode && status?.locked && (
            <button type="button" className="primary" onClick={() => setAction("unlock")}>
              {t("vault.action.unlock")}
            </button>
          )}
          {masterMode && !status?.locked && (
            <button type="button" onClick={() => void lock()}>
              {t("vault.lock")}
            </button>
          )}
        </div>
      </fieldset>

      <fieldset className="settings-group">
        <legend>{t("vault.securityTitle")}</legend>
        <div className="plugin-actions">
          <button
            type="button"
            className="primary"
            onClick={() => setAction(masterMode ? "change" : "enable")}
            disabled={!isTauri}
          >
            {masterMode
              ? t("vault.action.change")
              : t("vault.enableMasterPassword")}
          </button>
          {masterMode && (
            <button type="button" onClick={() => setAction("disable")}>
              {t("vault.action.disable")}
            </button>
          )}
        </div>
        {!masterMode && <p className="hint">{t("vault.masterHint")}</p>}
        <SelectSetting
          title={t("vault.autoLock")}
          hint={t("vault.autoLockHint")}
          disabled={!masterMode}
          value={String(autoLockMinutes)}
          onChange={(value) => setAutoLockMinutes(Number(value))}
          options={AUTO_LOCK_CHOICES.map((value) => ({
            value: String(value),
            label:
              value === 0
                ? t("vault.autoLockOff")
                : t("vault.autoLockMinutes", { count: value }),
          }))}
        />
      </fieldset>

      <fieldset className="settings-group">
        <legend>{t("vault.keysSectionTitle")}</legend>
        <div className="plugin-actions">
          <button type="button" onClick={() => setKeyManagerOpen(true)}>
            {t("keys.manage")}
          </button>
          <button type="button" onClick={() => setKnownHostsOpen(true)}>
            {t("knownHosts.manage")}
          </button>
        </div>
      </fieldset>

      <fieldset className="settings-group">
        <legend>{t("vault.libraryTitle")}</legend>
        {entries.length === 0 ? (
          <p className="hint">{t("vault.entriesEmpty")}</p>
        ) : (
          <>
            <ul className="plugin-list">
              {entries.map((entry) => (
                <li key={entry.secretRef} className="plugin-row">
                  <span className="plugin-name">
                    {entry.label || entry.secretRef}
                  </span>
                  <span className="plugin-kind">{entry.kind}</span>
                  <button
                    type="button"
                    className="link-btn danger"
                    onClick={() => void deleteEntry(entry)}
                  >
                    {t("common.delete")}
                  </button>
                </li>
              ))}
            </ul>
            {confirmingClear ? (
              <div className="plugin-actions">
                <button type="button" className="danger" onClick={() => void clearEntries()}>
                  {t("vault.clearConfirm")}
                </button>
                <button type="button" onClick={() => setConfirmingClear(false)}>
                  {t("common.cancel")}
                </button>
              </div>
            ) : (
              <div className="plugin-actions">
                <button type="button" onClick={() => setConfirmingClear(true)}>
                  {t("vault.clearAll")}
                </button>
              </div>
            )}
          </>
        )}
        {masterMode && (
          <div className="plugin-actions">
            <button type="button" className="danger" onClick={() => setAction("reset")}>
              {t("vault.action.reset")}
            </button>
          </div>
        )}
        <p className="hint">{t("vault.resetHint")}</p>
      </fieldset>

      <fieldset className="settings-group">
        <legend>{t("vault.auditTitle")}</legend>
        {audit.length === 0 ? (
          <p className="hint">{t("vault.auditEmpty")}</p>
        ) : (
          <ul className="vault-audit">
            {groupByDay(audit, (record) => parseStamp(record.at)).map(
              (group) => (
                <Fragment key={group.key}>
                  <DaySeparator
                    as="li"
                    sticky
                    label={group.key === "unknown" ? t("time.unknownDay") : dayLabel(group.date)}
                    count={t("time.dayCount", { count: group.items.length })}
                  />
                  {group.items.map((record) => {
                    const date = parseStamp(record.at);
                    return (
                      <li key={record.id} className="vault-audit-row">
                        <span className="vault-audit-time">
                          {date ? clockLabel(date, true) : record.at}
                        </span>
                        <span className="vault-audit-action">
                          {record.action}
                        </span>
                        <span
                          className="vault-audit-target"
                          title={record.detail}
                        >
                          {record.target}
                        </span>
                      </li>
                    );
                  })}
                </Fragment>
              ),
            )}
          </ul>
        )}
        <div className="plugin-actions">
          <button type="button" onClick={() => void refresh()}>
            {t("knownHosts.refresh")}
          </button>
        </div>
      </fieldset>

      {message && <p className="hint sync-message">{message}</p>}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}

      {action && (
        <UnlockDialog mode={action} onClose={() => setAction(null)} onDone={finish} />
      )}
      {keyManagerOpen && (
        <KeyManagerDialog
          onClose={() => {
            setKeyManagerOpen(false);
            void refresh();
          }}
        />
      )}
      {knownHostsOpen && (
        <KnownHostsDialog onClose={() => setKnownHostsOpen(false)} />
      )}
    </>
  );
}
