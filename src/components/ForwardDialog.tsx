import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  isTauri,
  type ForwardInfo,
  type ForwardKind,
  type ForwardRule,
} from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";
import { StopIcon, TrashIcon } from "./icons";

function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function ForwardDialog() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const setForwardOpen = useAppStore((s) => s.setForwardOpen);
  const [forwards, setForwards] = useState<ForwardInfo[]>([]);
  const [kind, setKind] = useState<ForwardKind>("local");
  const [sessionId, setSessionId] = useState("");
  const [bindHost, setBindHost] = useState("127.0.0.1");
  const [bindPort, setBindPort] = useState("8080");
  const [destHost, setDestHost] = useState("");
  const [destPort, setDestPort] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saveRule, setSaveRule] = useState(false);
  const [rules, setRules] = useState<ForwardRule[]>([]);

  const sshSessions = useMemo(
    () => sessions.filter((session) => session.kind === "ssh" && !session.closed),
    [sessions],
  );
  const defaultSession =
    sshSessions.find((session) => session.id === activeId)?.id ??
    sshSessions[0]?.id ??
    "";
  const chosenSession = sessionId || defaultSession;
  const hostId =
    sshSessions.find((session) => session.id === chosenSession)?.hostId ?? "";

  const loadRules = useCallback(async () => {
    if (!hostId) {
      setRules([]);
      return;
    }
    try {
      setRules(await api.listForwardRules(hostId));
    } catch {
      setRules([]);
    }
  }, [hostId]);

  useEffect(() => {
    if (!isTauri) return;
    void loadRules();
  }, [loadRules]);

  const load = useCallback(async () => {
    try {
      setForwards(await api.forwardList());
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    void load();
    const timer = window.setInterval(() => void load(), 2000);
    return () => window.clearInterval(timer);
  }, [load]);

  const close = () => setForwardOpen(false);

  const start = async () => {
    const local = Number.parseInt(bindPort, 10);
    const remote = kind === "dynamic" ? 0 : Number.parseInt(destPort, 10);
    if (!Number.isInteger(local) || local < 1 || local > 65535) {
      setError(t("forward.invalidPort"));
      return;
    }
    if (kind !== "dynamic" && (!Number.isInteger(remote) || remote < 1 || remote > 65535)) {
      setError(t("forward.invalidPort"));
      return;
    }
    if (kind !== "dynamic" && !destHost.trim()) {
      setError(t("forward.destRequired"));
      return;
    }
    if (!chosenSession) {
      setError(t("forward.noSession"));
      return;
    }
    setBusy(true);
    setError("");
    try {
      const info = await api.forwardStart({
        id: crypto.randomUUID(),
        sessionId: chosenSession,
        kind,
        bindHost: bindHost.trim() || undefined,
        bindPort: local,
        destHost: kind === "dynamic" ? "" : destHost.trim(),
        destPort: remote,
      });
      if (kind !== "dynamic") {
        setDestHost("");
        setDestPort("");
      }
      if (saveRule && hostId) {
        await api
          .saveForwardRule({
            id: `${hostId}:${kind}:${local}`,
            hostId,
            kind,
            bindHost: bindHost.trim() || "127.0.0.1",
            bindPort: local,
            destHost: kind === "dynamic" ? "" : destHost.trim(),
            destPort: remote,
          })
          .catch((err) => setError(String(err)));
        await loadRules();
      }
      await load();
      useAppStore.getState().setStatus(
        t("forward.started", {
          local: `${info.bindHost}:${info.bindPort}`,
          remote:
            info.kind === "dynamic"
              ? "SOCKS5"
              : `${info.destHost}:${info.destPort}`,
        }),
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const stop = async (id: string) => {
    try {
      await api.forwardStop(id);
      await load();
    } catch (err) {
      setError(String(err));
    }
  };

  const deleteRule = async (id: string) => {
    try {
      await api.deleteForwardRule(id);
      await loadRules();
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <Modal title={t("forward.title")} onClose={close}>
      {!isTauri ? (
        <p className="hint">{t("forward.desktopOnly")}</p>
      ) : (
        <div className="forward-body">
          <p className="hint">{t("forward.hint")}</p>
          {sshSessions.length === 0 ? (
            <p className="hint">{t("forward.noSession")}</p>
          ) : (
            <div className="form-grid">
              <label className="field">
                {t("forward.kind")}
                <select
                  value={kind}
                  onChange={(e) => setKind(e.target.value as ForwardKind)}
                >
                  <option value="local">{t("forward.local")}</option>
                  <option value="remote">{t("forward.remote")}</option>
                  <option value="dynamic">{t("forward.dynamic")}</option>
                </select>
              </label>
              <label className="field">
                {t("forward.session")}
                <select
                  value={chosenSession}
                  onChange={(e) => setSessionId(e.target.value)}
                >
                  {sshSessions.map((session) => (
                    <option key={session.id} value={session.id}>
                      {session.title}
                    </option>
                  ))}
                </select>
              </label>
              <div className="field-row">
                <label className="field grow">
                  {t(
                    kind === "remote"
                      ? "forward.bindHostRemote"
                      : "forward.bindHost",
                  )}
                  <input
                    value={bindHost}
                    onChange={(e) => setBindHost(e.target.value)}
                  />
                </label>
                <label className="field grow">
                  {t(
                    kind === "remote"
                      ? "forward.bindPortRemote"
                      : "forward.bindPort",
                  )}
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    value={bindPort}
                    onChange={(e) => setBindPort(e.target.value)}
                  />
                </label>
              </div>
              {kind === "dynamic" && (
                <p className="hint">{t("forward.dynamicHint")}</p>
              )}
              {kind === "remote" && (
                <p className="hint">{t("forward.remoteHint")}</p>
              )}
              {kind !== "dynamic" && (
                <div className="field-row">
                  <label className="field grow">
                    {t(
                      kind === "remote"
                        ? "forward.destHostRemote"
                        : "forward.destHost",
                    )}
                    <input
                      value={destHost}
                      placeholder="127.0.0.1"
                      onChange={(e) => setDestHost(e.target.value)}
                    />
                  </label>
                  <label className="field grow">
                    {t(
                      kind === "remote"
                        ? "forward.destPortRemote"
                        : "forward.destPort",
                    )}
                    <input
                      type="number"
                      min={1}
                      max={65535}
                      value={destPort}
                      onChange={(e) => setDestPort(e.target.value)}
                    />
                  </label>
                </div>
              )}
              {hostId && (
                <>
                  <label className="checkbox">
                    <input
                      type="checkbox"
                      checked={saveRule}
                      onChange={(e) => setSaveRule(e.target.checked)}
                    />
                    <span>{t("forward.saveRule")}</span>
                  </label>
                  <p className="hint">{t("forward.saveRuleHint")}</p>
                </>
              )}
              <div className="actions">
                <button
                  type="button"
                  className="primary"
                  disabled={busy}
                  onClick={() => void start()}
                >
                  {t("forward.start")}
                </button>
              </div>
            </div>
          )}
          {error && (
            <p className="error-text" role="alert">
              {error}
            </p>
          )}
          <h4 className="forward-heading">{t("forward.savedRules")}</h4>
          {rules.length === 0 ? (
            <p className="hint">{t("forward.noRules")}</p>
          ) : (
            <div className="forward-list">
              {rules.map((rule) => (
                <div key={rule.id} className="forward-row">
                  <div className="forward-main">
                    <span className="forward-route">
                      {rule.bindHost}:{rule.bindPort}
                      {rule.kind === "remote" && (
                        <span className="forward-tag">
                          {" "}
                          {t("forward.remoteTag")}
                        </span>
                      )}{" "}
                      →{" "}
                      {rule.kind === "dynamic"
                        ? "SOCKS5"
                        : `${rule.destHost}:${rule.destPort}`}
                    </span>
                    <span className="forward-meta">
                      {t("forward.autoStart")}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="icon-btn"
                    title={t("forward.deleteRule")}
                    aria-label={t("forward.deleteRule")}
                    onClick={() => void deleteRule(rule.id)}
                  >
                    <TrashIcon size={12} />
                  </button>
                </div>
              ))}
            </div>
          )}
          <h4 className="forward-heading">{t("forward.active")}</h4>
          {forwards.length === 0 ? (
            <p className="hint">{t("forward.none")}</p>
          ) : (
            <div className="forward-list">
              {forwards.map((item) => (
                <div key={item.id} className="forward-row">
                  <div className="forward-main">
                    <span className="forward-route">
                      {item.bindHost}:{item.bindPort}
                      {item.kind === "remote" && (
                        <span className="forward-tag">
                          {" "}
                          {t("forward.remoteTag")}
                        </span>
                      )}{" "}
                      →{" "}
                      {item.kind === "dynamic"
                        ? "SOCKS5"
                        : `${item.destHost}:${item.destPort}`}
                    </span>
                    <span className="forward-meta">
                      {t("forward.connections", { count: item.connections })} · ↓{" "}
                      {formatBytes(item.bytesIn)} ↑ {formatBytes(item.bytesOut)}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="icon-btn"
                    title={t("forward.stop")}
                    aria-label={t("forward.stop")}
                    onClick={() => void stop(item.id)}
                  >
                    <StopIcon size={12} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
