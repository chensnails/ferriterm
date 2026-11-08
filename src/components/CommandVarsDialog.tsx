import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { renderTemplate, templateVariables } from "../commands";
import { Modal } from "./Modal";

interface CommandVarsDialogProps {
  name: string;
  template: string;
  onCancel: () => void;
  onInsert: (command: string) => void;
}

export function CommandVarsDialog({
  name,
  template,
  onCancel,
  onInsert,
}: CommandVarsDialogProps) {
  const { t } = useTranslation();
  const variables = templateVariables(template);
  const [values, setValues] = useState<Record<string, string>>({});
  const firstRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    firstRef.current?.focus();
  }, []);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    onInsert(renderTemplate(template, values));
  };

  return (
    <Modal
      title={t("commands.variablesTitle", { name })}
      onClose={onCancel}
      footer={
        <>
          <button type="button" onClick={onCancel}>
            {t("common.cancel")}
          </button>
          <button type="submit" form="command-vars-form" className="primary">
            {t("commands.insert")}
          </button>
        </>
      }
    >
      <form id="command-vars-form" className="settings-form" onSubmit={submit}>
        <p className="hint">{t("commands.variablesHint")}</p>
        {variables.map((variable, index) => (
          <label className="field" key={variable}>
            <span>{variable}</span>
            <input
              ref={index === 0 ? firstRef : undefined}
              value={values[variable] ?? ""}
              onChange={(event) =>
                setValues((current) => ({
                  ...current,
                  [variable]: event.target.value,
                }))
              }
              autoComplete="off"
              spellCheck={false}
            />
          </label>
        ))}
        <code className="command-preview">
          {renderTemplate(template, values) || template}
        </code>
      </form>
    </Modal>
  );
}
