import { useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { isTauri, type SshConnectArgs } from "../api";
import { useAppStore } from "../store";

type AuthMethod = "password" | "key" | "agent" | "keyboard-interactive";

interface ParsedTarget {
  host: string;
  username?: string;
  port?: number;
}

/** Accepts ssh://user@host:port, user@host:port or a plain host name. */
function parseSshTarget(raw: string): ParsedTarget {
  let value = raw.trim();
  if (!value) return { host: "" };
  if (value.startsWith("ssh://")) {
    try {
      const url = new URL(value);
      return {
        host: url.hostname,
        username: url.username ? decodeURIComponent(url.username) : undefined,
        port: url.port ? Number(url.port) : undefined,
      };
    } catch {
      value = value.slice("ssh://".length);
    }
  }
  const at = value.lastIndexOf("@");
  let username: string | undefined;
  if (at > 0) {
    username = value.slice(0, at);
    value = value.slice(at + 1);
  }
  const colon = value.indexOf(":");
  let port: number | undefined;
  if (
    colon > 0 &&
    colon === value.lastIndexOf(":") &&
    /^\d+$/.test(value.slice(colon + 1))
  ) {
    port = Number(value.slice(colon + 1));
    value = value.slice(0, colon);
  }
  return { host: value, username, port };
}

export function ConnectForm({
  heading = true,
  onConnected,
}: {
  heading?: boolean;
  onConnected?: () => void;
}) {
  const { t } = useTranslation();
  const formRef = useRef<HTMLFormElement>(null);
  const openSession = useAppStore((s) => s.openSession);
  const setStatus = useAppStore((s) => s.setStatus);
  const status = useAppStore((s) => s.status);
  const [auth, setAuth] = useState<AuthMethod>("password");

  const syncFromUri = (raw: string) => {
    const form = formRef.current;
    if (!form || !raw.trim().startsWith("ssh://")) return;
    const parsed = parseSshTarget(raw);
    if (parsed.username) {
      const input = form.elements.namedItem("username") as HTMLInputElement | null;
      if (input) input.value = parsed.username;
    }
    if (parsed.port) {
      const input = form.elements.namedItem("port") as HTMLInputElement | null;
      if (input) input.value = String(parsed.port);
    }
  };

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!isTauri || !formRef.current) return;
    const data = new FormData(formRef.current);
    const target = parseSshTarget(String(data.get("host") ?? ""));
    const host = target.host;
    const username =
      target.username ?? String(data.get("username") ?? "").trim();
    if (!host || !username) {
      setStatus(t("connect.invalidHost"), true);
      return;
    }
    const port = target.port ?? (Number(data.get("port")) || 22);
    const args: Omit<SshConnectArgs, "cols" | "rows"> = { host, port, username };
    if (auth === "agent") {
      args.agent = true;
    } else if (auth === "keyboard-interactive") {
      args.keyboardInteractive = true;
    } else if (auth === "key") {
      args.keyPath = String(data.get("keyPath") ?? "").trim() || undefined;
      const passphrase = String(data.get("keyPassphrase") ?? "");
      if (passphrase) args.keyPassphrase = passphrase;
    } else {
      args.password = String(data.get("password") ?? "");
    }
    setStatus(t("connect.connecting"));
    openSession("ssh", `${username}@${host}`, { type: "ssh", args });
    onConnected?.();
  };

  const handleLocal = () => {
    if (!isTauri) return;
    openSession("pty", t("session.localTitle"), { type: "pty" });
    onConnected?.();
  };

  return (
    <form id="conn-form" ref={formRef} autoComplete="off" onSubmit={handleSubmit}>
      {heading && <h2>{t("connect.title")}</h2>}
      <label>
        {t("connect.host")}
        <input
          name="host"
          placeholder={t("connect.hostPlaceholder")}
          onChange={(e) => syncFromUri(e.target.value)}
          required
        />
        <span className="hint">{t("connect.uriHint")}</span>
      </label>
      <div className="row">
        <label className="grow">
          {t("connect.port")}
          <input name="port" type="number" defaultValue={22} min={1} max={65535} />
        </label>
        <label className="grow">
          {t("connect.username")}
          <input name="username" placeholder={t("connect.usernamePlaceholder")} required />
        </label>
      </div>
      <label>
        {t("connect.authMethod")}
        <select
          name="authMethod"
          value={auth}
          onChange={(e) => setAuth(e.target.value as AuthMethod)}
        >
          <option value="password">{t("connect.authPassword")}</option>
          <option value="key">{t("connect.authKey")}</option>
          <option value="agent">{t("connect.authAgent")}</option>
          <option value="keyboard-interactive">
            {t("connect.authInteractive")}
          </option>
        </select>
      </label>
      {auth === "password" && (
        <label>
          {t("connect.password")}
          <input
            name="password"
            type="password"
            placeholder={t("connect.passwordPlaceholder")}
          />
        </label>
      )}
      {auth === "key" && (
        <>
          <label>
            {t("connect.keyPath")}
            <input
              name="keyPath"
              placeholder={t("connect.keyPathPlaceholder")}
              required
            />
          </label>
          <label>
            {t("connect.passphrase")}
            <input name="keyPassphrase" type="password" />
          </label>
        </>
      )}
      {auth === "agent" && <p className="hint">{t("connect.agentHint")}</p>}
      {auth === "keyboard-interactive" && (
        <p className="hint">{t("connect.interactiveHint")}</p>
      )}
      <div className="actions">
        <button type="submit" className="primary" disabled={!isTauri}>
          {t("connect.submit")}
        </button>
        <button type="button" onClick={handleLocal} disabled={!isTauri}>
          {t("connect.local")}
        </button>
      </div>
      <p id="form-status" role="status" className={status.isError ? "error" : ""}>
        {status.text}
      </p>
    </form>
  );
}
