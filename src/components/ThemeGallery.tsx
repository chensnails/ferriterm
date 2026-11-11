import { useRef, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { ImportedTheme } from "../scheme";
import type { ThemePack } from "../api";
import { BUILTIN_THEMES, type Theme } from "../theme";
import { CheckIcon } from "./icons";

/**
 * The theme picker as a wall of real swatches instead of a dropdown. Each
 * preview element carries the theme's own `data-theme`, so it is painted by
 * the same CSS blocks the app uses — imported and plugin themes overlay their
 * sanitized tokens on top of their base palette.
 */
export function ThemeGallery({
  theme,
  packs,
  imported,
  onSelect,
}: {
  theme: Theme;
  packs: ThemePack[];
  imported: ImportedTheme[];
  onSelect: (id: Theme) => void;
}) {
  const { t } = useTranslation();

  const cardStyle = (tokens?: Record<string, string>): CSSProperties | undefined =>
    tokens && Object.keys(tokens).length > 0
      ? (tokens as CSSProperties)
      : undefined;

  const baseLabel = (base: string) =>
    t(`settings.theme_${base === "light" ? "light" : "dark"}`);

  const renderCard = (
    id: string,
    name: string,
    base: string,
    tokens?: Record<string, string>,
  ) => {
    const active = theme === id;
    const previewTheme =
      tokens && Object.keys(tokens).length > 0
        ? base === "light"
          ? "light"
          : base === "dark"
            ? "dark"
            : base
        : id;
    return (
      <button
        key={id}
        type="button"
        className={`theme-card${active ? " active" : ""}`}
        aria-pressed={active}
        onClick={() => onSelect(id as Theme)}
      >
        <span
          className="theme-card-preview"
          data-theme={previewTheme}
          style={cardStyle(tokens)}
          aria-hidden
        >
          <span className="theme-card-titlebar">
            <span className="theme-card-chip" />
            <span className="theme-card-pill" />
          </span>
          <span className="theme-card-line" />
          <span className="theme-card-line short" />
        </span>
        <span className="theme-card-meta">
          <span className="theme-card-name">{name}</span>
          {active ? (
            <span className="theme-card-check">
              <CheckIcon size={12} />
            </span>
          ) : (
            <span>{baseLabel(base)}</span>
          )}
        </span>
      </button>
    );
  };

  return (
    <div className="theme-grid" role="radiogroup" aria-label={t("settings.themeTitle")}>
      {BUILTIN_THEMES.map((item) =>
        renderCard(item.id, t(`settings.theme_${item.id}`), item.base),
      )}
      {packs.map((pack) =>
        renderCard(pack.id, pack.name, pack.base, pack.tokens),
      )}
      {imported.map((item) =>
        renderCard(item.id, item.name, item.base, item.tokens),
      )}
    </div>
  );
}

/** Hidden-file input kept next to the gallery so "import" stays one click. */
export function ThemeImportButton({
  onFile,
  label,
}: {
  onFile: (file: File) => void;
  label: string;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  return (
    <>
      <button type="button" onClick={() => inputRef.current?.click()}>
        {label}
      </button>
      <input
        ref={inputRef}
        type="file"
        accept=".itermcolors,.toml,.tml"
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) onFile(file);
          event.target.value = "";
        }}
      />
    </>
  );
}
