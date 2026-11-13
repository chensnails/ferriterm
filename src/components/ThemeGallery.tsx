import { useRef, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { ImportedTheme } from "../scheme";
import type { ThemePack } from "../api";
import { BUILTIN_THEMES, type Theme, type ThemeCategory, type BuiltinTheme } from "../theme";
import { CheckIcon } from "./icons";

// The picker orders categories as: community dark, community light, then the
// first-party baseline and emulator colour tables. Within a category, themes
// keep their order in BUILTIN_THEMES.
const CATEGORY_ORDER: ThemeCategory[] = ["community-dark", "community-light", "native"];

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
    caption?: string,
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
            <span>{caption}</span>
          )}
        </span>
      </button>
    );
  };

  const grouped = new Map<ThemeCategory, BuiltinTheme[]>();
  for (const item of BUILTIN_THEMES) {
    const list = grouped.get(item.category) ?? [];
    list.push(item);
    grouped.set(item.category, list);
  }

  const hasUserThemes = packs.length > 0 || imported.length > 0;

  return (
    <div className="theme-gallery" role="radiogroup" aria-label={t("settings.themeTitle")}>
      {CATEGORY_ORDER.map((cat) => {
        const items = grouped.get(cat);
        if (!items || items.length === 0) return null;
        return (
          <section className="theme-category" key={cat} aria-label={t(`settings.theme_cat_${cat}`)}>
            <div className="theme-category-title">
              <span>{t(`settings.theme_cat_${cat}`)}</span>
              <span className="theme-category-count">{items.length}</span>
            </div>
            <div className="theme-grid">
              {items.map((item) =>
                renderCard(
                  item.id,
                  t(`settings.theme_${item.id}`),
                  item.base,
                  undefined,
                  t(`settings.theme_cat_${cat}`),
                ),
              )}
            </div>
          </section>
        );
      })}
      {hasUserThemes && (
        <div className="theme-grid">
          {packs.map((pack) =>
            renderCard(pack.id, pack.name, pack.base, pack.tokens, baseLabel(pack.base)),
          )}
          {imported.map((item) =>
            renderCard(item.id, item.name, item.base, item.tokens, baseLabel(item.base)),
          )}
        </div>
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
