import type { HistoryEntry } from "./store";

export function historyForHost(
  entries: HistoryEntry[],
  sessionId: string,
  hostId?: string,
): HistoryEntry[] {
  return entries.filter((entry) =>
    hostId ? entry.hostId === hostId : entry.sessionId === sessionId,
  );
}

// Most recent first; exact matches are excluded because there is nothing to
// complete.
export function historyMatches(
  entries: HistoryEntry[],
  input: string,
  limit = 5,
): HistoryEntry[] {
  if (!input.trim()) return [];
  const seen = new Set<string>();
  const out: HistoryEntry[] = [];
  for (const entry of entries) {
    if (entry.text.length <= input.length) continue;
    if (!entry.text.startsWith(input)) continue;
    if (seen.has(entry.text)) continue;
    seen.add(entry.text);
    out.push(entry);
    if (out.length >= limit) break;
  }
  return out;
}