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
import { PasswordManagerDialog } from "./PasswordManagerDialog";
import { SelectSetting } from "./SettingsControls";
import { DaySeparator, Notice, SettingGroup, SettingRow, Skeleton } from "./ui";
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
  const [passwordManagerOpen, setPasswordManagerOpen] = useState(false);
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
      {/* Feedback belongs at the top of the pane: the action that produced it
          can be anywhere in a long list, and a message appended after the audit
          log used to land off-screen. */}
      {error && (
        <Notice tone="danger" className="vault-feedback">
          {error}
        </Notice>
      )}
      {message && !error && (
        <Notice className="vault-feedback">{message}</Notice>
      )}

      <SettingGroup title={t("vault.overviewTitle")}>
        {!isTauri && (
          <p className="ui-setting-group-hint">{t("vault.desktopOnly")}</p>
        )}
        <SettingRow
          title={
            <span className="vault-state">
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
            </span>
          }
          hint={t(masterMode ? "vault.modeMaster" : "vault.modeLocal")}
        >
          <span className="vault-count">
            {t("vault.entriesCount", { count: status?.entries ?? 0 })}
          </span>
        </SettingRow>
        {masterMode && (
          <div className="plugin-actions">
            {status?.locked ? (
              <button
                type="button"
                className="primary"
                onClick={() => setAction("unlock")}
              >
                {t("vault.action.unlock")}
              </button>
            ) : (
              <button type="button" onClick={() => void lock()}>
                {t("vault.lock")}
              </button>
            )}
          </div>
        )}
      </SettingGroup>

      <SettingGroup title={t("vault.securityTitle")}>
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
        {!masterMode && (
          <p className="ui-setting-group-hint">{t("vault.masterHint")}</p>
        )}
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
      </SettingGroup>

      <SettingGroup title={t("vault.keysSectionTitle")}>
        <div className="plugin-actions">
          <button type="button" onClick={() => setKeyManagerOpen(true)}>
            {t("keys.manage")}
          </button>
          <button type="button" onClick={() => setPasswordManagerOpen(true)}>
            {t("passwords.manage")}
          </button>
          <button type="button" onClick={() => setKnownHostsOpen(true)}>
            {t("knownHosts.manage")}
          </button>
        </div>
      </SettingGroup>

      <SettingGroup title={t("vault.libraryTitle")}>
        {entries.length === 0 ? (
          <p className="ui-setting-group-hint">{t("vault.entriesEmpty")}</p>
        ) : (
          <ul className="vault-list">
            {entries.map((entry) => (
              <li
                key={entry.secretRef}
                className="vault-row"
                title={entry.secretRef}
              >
                <span className="vault-row-name">
                  {entry.label || entry.secretRef}
                </span>
                <span className="vault-row-kind">
                  {t(`vault.kind.${entry.kind}`, { defaultValue: entry.kind })}
                </span>
                <button
                  type="button"
                  className="link-btn danger vault-row-action"
                  onClick={() => void deleteEntry(entry)}
                >
                  {t("common.delete")}
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="plugin-actions">
          {confirmingClear ? (
            <>
              <button
                type="button"
                className="danger"
                onClick={() => void clearEntries()}
              >
                {t("vault.clearConfirm")}
              </button>
              <button type="button" onClick={() => setConfirmingClear(false)}>
                {t("common.cancel")}
              </button>
            </>
          ) : (
            <button type="button" onClick={() => setConfirmingClear(true)}>
              {t("vault.clearAll")}
            </button>
          )}
          {masterMode && (
            <button
              type="button"
              className="danger"
              onClick={() => setAction("reset")}
            >
              {t("vault.action.reset")}
            </button>
          )}
        </div>
        <p className="ui-setting-group-hint">{t("vault.resetHint")}</p>
      </SettingGroup>

      <SettingGroup title={t("vault.auditTitle")}>
        {audit.length === 0 ? (
          <p className="ui-setting-group-hint">{t("vault.auditEmpty")}</p>
        ) : (
          <ul className="vault-audit">
            {groupByDay(audit, (record) => parseStamp(record.at)).map(
              (group) => (
                <Fragment key={group.key}>
                  <DaySeparator
                    as="li"
                    sticky
                    label={
                      group.key === "unknown"
                        ? t("time.unknownDay")
                        : dayLabel(group.date)
                    }
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
      </SettingGroup>

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
      {passwordManagerOpen && (
        <PasswordManagerDialog
          onClose={() => {
            setPasswordManagerOpen(false);
            void refresh();
          }}
        />
      )}
    </>
  );
}
