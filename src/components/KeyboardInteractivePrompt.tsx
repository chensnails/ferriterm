import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { api, type KeyboardInteractiveChallenge } from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

export function KeyboardInteractivePrompt() {
  const { t } = useTranslation();
  const [request, setRequest] = useState<KeyboardInteractiveChallenge | null>(null);
  const [values, setValues] = useState<string[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<KeyboardInteractiveChallenge>(
      "auth://keyboard-interactive",
      (event) => {
        if (disposed) return;
        const owns = useAppStore
          .getState()
          .sessions.some((session) => session.id === event.payload.sessionId);
        if (!owns) return;
        setError("");
        setValues(event.payload.prompts.map(() => ""));
        setRequest(event.payload);
      },
    ).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const respond = async (answers: string[] | null) => {
    const current = request;
    if (!current) return;
    setRequest(null);
    setError("");
    try {
      await api.sshKeyboardInteractiveRespond(current.requestId, answers);
    } catch (err) {
      setError(String(err));
    }
  };

  if (!request) return null;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    void respond(values);
  };

  return (
    <Modal
      title={request.name || t("keyboardAuth.title")}
      onClose={() => void respond(null)}
      footer={
        <>
          <button type="button" onClick={() => void respond(null)}>
            {t("keyboardAuth.cancel")}
          </button>
          <button
            type="submit"
            form="keyboard-auth-form"
            className="primary"
          >
            {t("keyboardAuth.submit")}
          </button>
        </>
      }
    >
      <form id="keyboard-auth-form" className="form-grid" onSubmit={submit}>
        {request.instructions && (
          <p className="hint keyboard-instructions">{request.instructions}</p>
        )}
        {request.prompts.map((prompt, index) => (
          <label key={index} className="field">
            <span>
              {prompt.text || t("keyboardAuth.prompt", { index: index + 1 })}
            </span>
            <input
              autoFocus={index === 0}
              type={prompt.echo ? "text" : "password"}
              autoComplete="off"
              value={values[index] ?? ""}
              onChange={(e) =>
                setValues((current) =>
                  current.map((value, i) => (i === index ? e.target.value : value)),
                )
              }
            />
          </label>
        ))}
        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
