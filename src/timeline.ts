import i18n from "./i18n";

/**
 * Day grouping for time-ordered lists (credential audit, version history).
 * Ages are shown relative to the reader's local calendar, so the separators
 * have to bucket by local day rather than by whatever wall clock the row
 * happens to carry.
 */

const PAD = (value: number) => String(value).padStart(2, "0");

/**
 * Parses the two timestamp shapes used by the app: SQLite `datetime('now')`
 * ("YYYY-MM-DD HH:MM:SS", always UTC and zone-less) and git ISO output
 * ("YYYY-MM-DDTHH:mm:ss+08:00"). Zone-less values are read as UTC because that
 * is what the audit table stores; treating them as local would shift every
 * audit row by the machine's offset and put it in the wrong day.
 */
export function parseStamp(value: string): Date | null {
  if (!value) return null;
  const iso = value.includes("T") ? value : value.replace(" ", "T");
  const zoned = /([zZ]|[+-]\d{2}:?\d{2})$/.test(iso) ? iso : `${iso}Z`;
  const date = new Date(zoned);
  return Number.isNaN(date.getTime()) ? null : date;
}

/** Local calendar day as YYYY-MM-DD, used as the grouping key. */
export function dayKey(date: Date): string {
  return `${date.getFullYear()}-${PAD(date.getMonth() + 1)}-${PAD(date.getDate())}`;
}

function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

/** "今天" / "昨天" / "2026-11-08 周六" for a group heading. */
export function dayLabel(date: Date): string {
  const today = startOfDay(new Date());
  const target = startOfDay(date);
  const days = Math.round((today.getTime() - target.getTime()) / 86400000);
  if (days === 0) return i18n.t("time.today");
  if (days === 1) return i18n.t("time.yesterday");
  const stamp = dayKey(date);
  const weekday = new Intl.DateTimeFormat(i18n.language, {
    weekday: "short",
  }).format(date);
  return `${stamp} ${weekday}`;
}

/** HH:mm:ss (with `seconds`) or HH:mm, in the reader's timezone. */
export function clockLabel(date: Date, seconds = false): string {
  const base = `${PAD(date.getHours())}:${PAD(date.getMinutes())}`;
  return seconds ? `${base}:${PAD(date.getSeconds())}` : base;
}

export interface DayGroup<T> {
  key: string;
  /** First timestamp of the bucket; rows with an unparsable stamp use epoch. */
  date: Date;
  items: T[];
}

/**
 * Buckets already-ordered rows into runs of the same local day. Rows whose
 * stamp cannot be parsed keep their order in a trailing "unknown" bucket
 * instead of disappearing.
 */
export function groupByDay<T>(
  items: T[],
  stamp: (item: T) => Date | null,
): DayGroup<T>[] {
  const groups: DayGroup<T>[] = [];
  for (const item of items) {
    const date = stamp(item);
    const key = date ? dayKey(date) : "unknown";
    const last = groups[groups.length - 1];
    if (last && last.key === key) last.items.push(item);
    else groups.push({ key, date: date ?? new Date(0), items: [item] });
  }
  return groups;
}
