import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { Tag } from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";
import { TAG_COLORS, TagPalette } from "./TagPalette";

function friendlyError(err: unknown, duplicate: string): string {
  const text = String(err);
  return text.includes("UNIQUE constraint failed: tags.name") ? duplicate : text;
}

function TagRow({ tag, onError }: { tag: Tag; onError: (message: string) => void }) {
  const { t } = useTranslation();
  const updateTag = useAppStore((s) => s.updateTag);
  const deleteTag = useAppStore((s) => s.deleteTag);
  const [name, setName] = useState(tag.name);
  const [color, setColor] = useState(tag.color);
  const [confirming, setConfirming] = useState(false);

  useEffect(() => setName(tag.name), [tag.name]);
  useEffect(() => setColor(tag.color), [tag.color]);

  const commitName = async () => {
    const value = name.trim();
    if (!value || value === tag.name) {
      setName(tag.name);
      return;
    }
    try {
      await updateTag({ ...tag, name: value, color });
    } catch (err) {
      setName(tag.name);
      onError(friendlyError(err, t("tags.duplicate")));
    }
  };

  const applyColor = (next: string) => {
    if (next === tag.color) return;
    setColor(next);
    updateTag({ ...tag, name: name.trim() || tag.name, color: next }).catch((err) =>
      onError(String(err)),
    );
  };

  if (confirming) {
    return (
      <li className="tag-row">
        <span className="tag-confirm">
          <span>{t("tags.deleteConfirm", { name: tag.name })}</span>
          <button
            type="button"
            className="danger"
            onClick={() => {
              deleteTag(tag.id).catch((err) => onError(String(err)));
              setConfirming(false);
            }}
          >
            {t("common.delete")}
          </button>
          <button type="button" onClick={() => setConfirming(false)}>
            {t("tags.cancel")}
          </button>
        </span>
      </li>
    );
  }

  return (
    <li className="tag-row">
      <input
        type="text"
        value={name}
        aria-label={t("tags.name")}
        onChange={(e) => setName(e.target.value)}
        onBlur={() => void commitName()}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.currentTarget.blur();
          }
        }}
      />
      <TagPalette value={color} onSelect={applyColor} />
      <button
        type="button"
        className="icon-btn"
        title={t("tags.delete")}
        aria-label={t("tags.delete")}
        onClick={() => setConfirming(true)}
      >
        ×
      </button>
    </li>
  );
}

export function TagManagerDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const tags = useAppStore((s) => s.tags);
  const createTag = useAppStore((s) => s.createTag);
  const [name, setName] = useState("");
  const [color, setColor] = useState<string>(TAG_COLORS[0]);
  const [error, setError] = useState("");

  const add = async (e: FormEvent) => {
    e.preventDefault();
    const value = name.trim();
    if (!value) return;
    setError("");
    try {
      await createTag({ id: crypto.randomUUID(), name: value, color });
      setName("");
    } catch (err) {
      setError(friendlyError(err, t("tags.duplicate")));
    }
  };

  return (
    <Modal title={t("tags.title")} onClose={onClose}>
      <ul className="tag-list">
        {tags.length === 0 && <li className="hint">{t("tags.empty")}</li>}
        {tags.map((tag) => (
          <TagRow key={tag.id} tag={tag} onError={setError} />
        ))}
      </ul>
      <form className="tag-create" onSubmit={add}>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("tags.newPlaceholder")}
          aria-label={t("tags.name")}
        />
        <TagPalette value={color} onSelect={setColor} />
        <button type="submit" className="primary" disabled={!name.trim()}>
          {t("tags.add")}
        </button>
      </form>
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
