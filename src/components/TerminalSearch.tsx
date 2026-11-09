import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { terminalHandle, type SearchResult } from "../terminal-registry";
import { useAppStore } from "../store";
import { CloseIcon, Chevron } from "./icons";

export function TerminalSearch() {
  const { t } = useTranslation();
  const searchOpen = useAppStore((s) => s.searchOpen);
  const closeSearch = useAppStore((s) => s.closeSearch);
  const activeId = useAppStore((s) => s.activeId);
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [regex, setRegex] = useState(false);
  const [result, setResult] = useState<SearchResult>({ index: -1, count: 0 });

  useEffect(() => {
    if (!searchOpen) return;
    const raf = requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
    return () => cancelAnimationFrame(raf);
  }, [searchOpen]);

  useEffect(() => {
    setQuery("");
    setRegex(false);
    setResult({ index: -1, count: 0 });
    if (activeId) terminalHandle(activeId)?.resetSearch();
  }, [activeId]);

  if (!searchOpen) return null;

  const view = activeId ? terminalHandle(activeId) : null;

  const run = (value: string, backwards: boolean, useRegex = regex) => {
    if (!view) return;
    setResult(view.find(value, backwards, useRegex));
  };

  const dismiss = () => {
    view?.resetSearch();
    closeSearch();
  };

  return (
    <div className="terminal-search">
      <input
        ref={inputRef}
        value={query}
        placeholder={t("search.placeholder")}
        aria-label={t("search.placeholder")}
        onChange={(event) => {
          setQuery(event.target.value);
          run(event.target.value, false);
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            dismiss();
          } else if (event.key === "Enter") {
            event.preventDefault();
            run(query, event.shiftKey);
          }
        }}
      />
      <button
        className="icon-btn"
        title={t("search.regex")}
        aria-label={t("search.regex")}
        aria-pressed={regex}
        onClick={() => {
          const next = !regex;
          setRegex(next);
          run(query, false, next);
        }}
      >
        .*
      </button>
      <span className="terminal-search-count">
        {result.error
          ? t("search.invalid")
          : result.count > 0
            ? t("search.count", { index: result.index + 1, count: result.count })
            : t("search.none")}
      </span>
      <button
        className="icon-btn"
        title={t("search.previous")}
        aria-label={t("search.previous")}
        onClick={() => run(query, true)}
        disabled={!view || result.count === 0}
      >
        <Chevron open={false} size={11} />
      </button>
      <button
        className="icon-btn"
        title={t("search.next")}
        aria-label={t("search.next")}
        onClick={() => run(query, false)}
        disabled={!view || result.count === 0}
      >
        <Chevron open size={11} />
      </button>
      <button
        className="icon-btn"
        title={t("search.close")}
        aria-label={t("search.close")}
        onClick={dismiss}
      >
        <CloseIcon size={12} />
      </button>
    </div>
  );
}
