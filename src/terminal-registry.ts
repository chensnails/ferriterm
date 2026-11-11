export interface SearchResult {
  index: number;
  count: number;
  error?: boolean;
}

export interface TerminalHandle {
  readonly term: { cols: number; rows: number };
  insert(text: string): void;
  find(query: string, backwards?: boolean, regex?: boolean): SearchResult;
  resetSearch(): void;
  /** Last `max` visible/buffered lines, newest last. Used for AI context. */
  screenLines(max: number): string[];
  selectionText(): string;
}

const handles = new Map<string, TerminalHandle>();

export function registerTerminal(id: string, handle: TerminalHandle): void {
  handles.set(id, handle);
}

export function unregisterTerminal(id: string, handle: TerminalHandle): void {
  if (handles.get(id) === handle) handles.delete(id);
}

export function terminalHandle(id: string): TerminalHandle | null {
  return handles.get(id) ?? null;
}
