import type { Tag } from "../api";

export function TagChip({ tag, small = false }: { tag: Tag; small?: boolean }) {
  return (
    <span
      className={`tag-chip tag-${tag.color}${small ? " small" : ""}`}
      title={tag.name}
    >
      {tag.name}
    </span>
  );
}
