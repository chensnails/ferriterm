import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { agentCliPath, isTauri } from "../api";
import { writeClipboardText } from "../clipboard";
import { CopyIcon } from "./icons";
import { Notice, SettingGroup } from "./ui";

/**
 * Settings → Agent: what an external agent needs in order to use this app.
 *
 * Nothing is written on the user's behalf here. Every MCP client keeps its
 * configuration in a different file, in a different dialect of JSON, sometimes
 * with comments — guessing at that would be a good way to break someone's
 * editor. So this page hands over the two facts that are actually hard to get
 * right, the binary's real path and the shape of an entry, and lets the person
 * put them where their client wants them.
 */
export function SettingsAgent() {
  const { t } = useTranslation();
  const [cliPath, setCliPath] = useState("ferriterm-cli");
  const [copied, setCopied] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!isTauri) return;
    // The path on screen must be the one that actually runs, so it comes from
    // the process rather than from a guess about where the app was installed.
    void agentCliPath()
      .then(setCliPath)
      .catch(() => {});
  }, []);

  const snippet = JSON.stringify(
    { mcpServers: { ferriterm: { command: cliPath, args: ["mcp"] } } },
    null,
    2,
  );
  const status = `${cliPath} status`;

  const copy = async (key: string, text: string) => {
    try {
      await writeClipboardText(text);
      setCopied(key);
      setFailed(false);
      window.setTimeout(() => setCopied(null), 1600);
    } catch {
      setFailed(true);
    }
  };

  const copyButton = (key: string, text: string) => (
    <button
      type="button"
      className="link-btn agent-copy"
      onClick={() => void copy(key, text)}
    >
      <CopyIcon size={12} />
      {copied === key ? t("agent.settings.copied") : t("agent.settings.copy")}
    </button>
  );

  return (
    <>
      <SettingGroup
        title={t("agent.settings.cliGroup")}
        hint={t("agent.settings.cliHint")}
      >
        <div className="agent-block">
          <code className="agent-code">{cliPath}</code>
          {copyButton("cli", cliPath)}
        </div>
      </SettingGroup>

      <SettingGroup
        title={t("agent.settings.snippetGroup")}
        hint={t("agent.settings.snippetHint")}
      >
        <div className="agent-block agent-block-stack">
          <pre className="agent-code">{snippet}</pre>
          <div className="agent-block-foot">
            <span className="hint">{t("agent.settings.snippetFoot")}</span>
            {copyButton("snippet", snippet)}
          </div>
        </div>
      </SettingGroup>

      <SettingGroup
        title={t("agent.settings.verifyGroup")}
        hint={t("agent.settings.verifyHint")}
      >
        <div className="agent-block agent-block-stack">
          <code className="agent-code">{status}</code>
          <div className="agent-block-foot">
            <span className="hint">{t("agent.settings.verifyFoot")}</span>
            {copyButton("status", status)}
          </div>
        </div>
        {failed && <Notice tone="danger">{t("agent.settings.copyFailed")}</Notice>}
      </SettingGroup>
    </>
  );
}
