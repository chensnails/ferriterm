import { useRef, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { isTauri, type SshConnectArgs } from "../api";
import { useAppStore } from "../store";

export function ConnectForm() {
  const { t } = useTranslation();
  const formRef = useRef<HTMLFormElement>(null);
  const openSession = useAppStore((s) => s.openSession);
  const setStatus = useAppStore((s) => s.setStatus);
  const status = useAppStore((s) => s.status);

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!isTauri || !formRef.current) return;
    const data = new FormData(formRef.current);
    const host = String(data.get("host") ?? "").trim();
    const username = String(data.get("username") ?? "").trim();
    const args: Omit<SshConnectArgs, "cols" | "rows"> = {
      host,
      port: Number(data.get("port")) || 22,
      username,
      password: String(data.get("password") ?? ""),
    };
    setStatus(t("connect.connecting"));
    openSession("ssh", `${username}@${host}`, { type: "ssh", args });
  };

  const handleLocal = () => {
    if (!isTauri) return;
    openSession("pty", t("session.localTitle"), { type: "pty" });
  };

  return (
    <form id="conn-form" ref={formRef} autoComplete="off" onSubmit={handleSubmit}>
      <h2>{t("connect.title")}</h2>
      <label>
        {t("connect.host")}
        <input name="host" placeholder={t("connect.hostPlaceholder")} required />
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
        {t("connect.password")}
        <input
          name="password"
          type="password"
          placeholder={t("connect.passwordPlaceholder")}
        />
      </label>
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