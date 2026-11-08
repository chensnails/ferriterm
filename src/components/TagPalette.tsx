import { useTranslation } from "react-i18next";

export const TAG_COLORS = [
  "blue",
  "green",
  "amber",
  "red",
  "purple",
  "cyan",
  "pink",
  "gray",
] as const;

export type TagColor = (typeof TAG_COLORS)[number];

export function TagPalette({
  value,
  onSelect,
}: {
  value: string;
  onSelect: (color: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="tag-palette" role="group" aria-label={t("tags.color")}>
      {TAG_COLORS.map((color) => (
        <button
          key={color}
          type="button"
          className={`swatch tag-${color}${color === value ? " selected" : ""}`}
          title={color}
          aria-label={color}
          onClick={() => onSelect(color)}
        />
      ))}
    </div>
  );
}
