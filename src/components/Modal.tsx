import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { DialogHeader } from "./ui";

// The stylesheet gives every backdrop the same z-index, so two dialogs open at
// once — a confirm asked from inside Settings — would otherwise resolve by DOM
// order, which buries the newer one underneath. Dialogs are stacked here in
// mount order instead: the newest sits on top and is the only one that answers
// Escape, so closing a confirm no longer also closes the dialog that asked it.
const BASE_Z = 300;
let dialogSeq = 0;
const openDialogs: number[] = [];

function useDialogStack() {
  const idRef = useRef(0);
  if (idRef.current === 0) idRef.current = ++dialogSeq;
  const id = idRef.current;
  const [depth, setDepth] = useState(0);

  useEffect(() => {
    openDialogs.push(id);
    setDepth(openDialogs.length);
    return () => {
      const at = openDialogs.indexOf(id);
      if (at >= 0) openDialogs.splice(at, 1);
    };
  }, [id]);

  return { id, depth };
}

interface ModalProps {
  title: string;
  subtitle?: string;
  icon?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  className?: string;
}

export function Modal({
  title,
  subtitle,
  icon,
  onClose,
  children,
  footer,
  wide,
  className,
}: ModalProps) {
  const { t } = useTranslation();
  const dialogRef = useRef<HTMLDivElement>(null);
  const { id, depth } = useDialogStack();

  // Move focus into the dialog so Tab starts inside it and Escape is not the
  // only keyboard path out of the window behind it.
  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (openDialogs[openDialogs.length - 1] !== id) return;
      onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, id]);

  return (
    <div
      className="modal-backdrop"
      style={{ zIndex: BASE_Z + depth }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        className={["modal", wide ? "wide" : "", className ?? ""]
          .filter(Boolean)
          .join(" ")}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
      >
        <DialogHeader
          title={title}
          subtitle={subtitle}
          icon={icon}
          closeLabel={t("common.close")}
          onClose={onClose}
        />
        <div className="modal-body">{children}</div>
        {footer && <footer className="modal-foot">{footer}</footer>}
      </div>
    </div>
  );
}
