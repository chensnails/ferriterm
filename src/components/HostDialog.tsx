import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { HostEntry, HostInput } from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";
import { TagChip } from "./TagChip";

interface HostDialogProps {
  host?: HostEntry;
  parentId: string | null;
  isFolder: boolean;
  onClose: () => void;
}

export function HostDialog({ host, parentId, isFolder, onClose }: HostDialogProps) {
  const { t } = useTranslation();
  const tags = useAppStore((s) => s.tags);
  const hosts = useAppStore((s) => s.hosts);
  const createHost = useAppStore((s) => s.createHost);
  const updateHost = useAppStore((s) => s.updateHost);

  const [name, setName] = useState(host?.name ?? "");
  const [address, setAddress] = useState(host?.host ?? "");
  const [port, setPort] = useState(String(host?.port ?? 22));
  const [username, setUsername] = useState(host?.username ?? "");
  const [authMethod, setAuthMethod] = useState(host?.authMethod ?? "password");
  const [keyPath, setKeyPath] = useState(host?.keyPath ?? "");
  const [jumpHostId, setJumpHostId] = useState(host?.jumpHostId ?? "");
  const [x11, setX11] = useState(host?.x11 ?? false);
  const [production, setProduction] = useState(host?.production ?? false);
  const [notes, setNotes] = useState(host?.notes ?? "");
  const [tagIds, setTagIds] = useState<string[]>(host?.tags.map((tag) => tag.id) ?? []);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const title = host
    ? isFolder
      ? t("hostDialog.editFolderTitle")
      : t("hostDialog.editHostTitle")
    : isFolder
      ? t("hostDialog.newFolderTitle")
      : t("hostDialog.newHostTitle");

  const toggleTag = (id: string) =>
    setTagIds((ids) => (ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]));

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    setSaving(true);
    const input: HostInput = {
      id: host?.id ?? crypto.randomUUID(),
      name: name.trim(),
      host: isFolder ? "" : address.trim(),
      port: Number(port) || 22,
      username: isFolder ? "" : username.trim(),
      authMethod: isFolder ? "password" : authMethod,
      keyPath: isFolder || authMethod !== "key" ? "" : keyPath.trim(),
      jumpHostId: isFolder ? "" : jumpHostId,
      x11: isFolder ? false : x11,
      parentId: host ? host.parentId : parentId,
      isFolder,
      production: isFolder ? false : production,
      notes: isFolder ? "" : notes.trim(),
      tagIds: isFolder ? [] : tagIds,
    };
    try {
      if (host) await updateHost(input);
      else await createHost(input);
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="submit"
            form="host-dialog-form"
            className="primary"
            disabled={saving}
          >
            {saving ? t("hostDialog.saving") : t("hostDialog.save")}
          </button>
        </>
      }
    >
      <form id="host-dialog-form" className="form-grid" onSubmit={submit}>
        <label className="field">
          <span>{t("hostDialog.name")}</span>
          <input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("hostDialog.namePlaceholder")}
            required
          />
        </label>

        {isFolder ? (
          <p className="hint">{t("hostDialog.folderHint")}</p>
        ) : (
          <>
            <label className="field">
              <span>{t("hostDialog.host")}</span>
              <input
                value={address}
                onChange={(e) => setAddress(e.target.value)}
                placeholder={t("connect.hostPlaceholder")}
                required
              />
            </label>
            <div className="field-row">
              <label className="field">
                <span>{t("hostDialog.port")}</span>
                <input
                  type="number"
                  min={1}
                  max={65535}
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                />
              </label>
              <label className="field grow">
                <span>{t("hostDialog.username")}</span>
                <input
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder={t("connect.usernamePlaceholder")}
                  required
                />
              </label>
            </div>

            <label className="field">
              <span>{t("hostDialog.authMethod")}</span>
              <select
                value={authMethod}
                onChange={(e) => setAuthMethod(e.target.value)}
              >
                <option value="password">{t("hostDialog.authPassword")}</option>
                <option value="key">{t("hostDialog.authKey")}</option>
                <option value="agent">{t("hostDialog.authAgent")}</option>
                <option value="keyboard-interactive">
                  {t("hostDialog.authKeyboardInteractive")}
                </option>
              </select>
            </label>

            {authMethod === "agent" && (
              <p className="hint">{t("hostDialog.agentHint")}</p>
            )}

            {authMethod === "keyboard-interactive" && (
              <p className="hint">{t("hostDialog.keyboardInteractiveHint")}</p>
            )}

            {authMethod === "key" && (
              <label className="field">
                <span>{t("hostDialog.keyPath")}</span>
                <input
                  value={keyPath}
                  onChange={(e) => setKeyPath(e.target.value)}
                  placeholder="~/.ssh/id_ed25519"
                />
              </label>
            )}

            <label className="field">
              <span>{t("hostDialog.jumpHost")}</span>
              <select
                value={jumpHostId}
                onChange={(e) => setJumpHostId(e.target.value)}
              >
                <option value="">{t("hostDialog.jumpNone")}</option>
                {hosts
                  .filter(
                    (item) => !item.isFolder && item.id !== host?.id,
                  )
                  .map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name || `${item.username}@${item.host}`}
                    </option>
                  ))}
              </select>
              <span className="hint">{t("hostDialog.jumpHint")}</span>
            </label>

            <label className="checkbox">
              <input
                type="checkbox"
                checked={x11}
                onChange={(e) => setX11(e.target.checked)}
              />
              <span>{t("hostDialog.x11")}</span>
            </label>
            <p className="hint">{t("hostDialog.x11Hint")}</p>

            <label className="field">
              <span>{t("hostDialog.notes")}</span>
              <textarea
                rows={3}
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                placeholder={t("hostDialog.notesPlaceholder")}
              />
            </label>

            <fieldset className="tag-picker">
              <legend>{t("hostDialog.tags")}</legend>
              {tags.length === 0 ? (
                <p className="hint">{t("hostDialog.noTags")}</p>
              ) : (
                <div className="tag-options">
                  {tags.map((tag) => (
                    <label key={tag.id} className="tag-option">
                      <input
                        type="checkbox"
                        checked={tagIds.includes(tag.id)}
                        onChange={() => toggleTag(tag.id)}
                      />
                      <TagChip tag={tag} />
                    </label>
                  ))}
                </div>
              )}
            </fieldset>

            <label className="checkbox">
              <input
                type="checkbox"
                checked={production}
                onChange={(e) => setProduction(e.target.checked)}
              />
              <span>{t("hostDialog.production")}</span>
            </label>
            <p className="hint">{t("hostDialog.productionHint")}</p>
          </>
        )}

        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
