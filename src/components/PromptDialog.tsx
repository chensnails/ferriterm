import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

/**
 * One dialog for both questions the file pane asks: "delete this?" and
 * "what should it be called?". The embedded WebView does not implement
 * `window.confirm` / `window.prompt`, so routing them through here is what
 * makes those actions work at all, and it keeps the look consistent.
 */
export function PromptDialog() {
  const { t } = useTranslation();
  const pending = useAppStore((s) => s.appPrompt);
  const settle = useAppStore((s) => s.settlePrompt);
  const request = pending?.request;
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (request?.kind !== "text") return;
    setValue(request.value);
    const id = window.setTimeout(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    }, 0);
    return () => window.clearTimeout(id);
  }, [request]);

  // The approve prompt has its own dialog; this one only owns the two
  // questions the file pane asks.
  if (!request || request.kind === "approve") return null;

  const cancel = () => settle(request.kind === "text" ? null : false);
  const confirm = () => settle(request.kind === "text" ? value.trim() : true);
  const trimmed = value.trim();

  return (
    <Modal
      title={request.title}
      onClose={cancel}
      footer={
        <>
          <button type="button" onClick={cancel}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className={
              request.kind === "confirm" && request.danger ? "danger" : "primary"
            }
            disabled={request.kind === "text" && trimmed.length === 0}
            onClick={confirm}
          >
            {request.kind === "confirm" && request.confirmLabel
              ? request.confirmLabel
              : t("common.confirm")}
          </button>
        </>
      }
    >
      {request.body && <p className="prompt-body">{request.body}</p>}
      {request.kind === "text" && (
        <input
          ref={inputRef}
          className="prompt-input"
          value={value}
          placeholder={request.placeholder ?? ""}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            if (trimmed.length > 0) confirm();
          }}
        />
      )}
    </Modal>
  );
}
