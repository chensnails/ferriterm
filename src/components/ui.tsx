import type {
  ButtonHTMLAttributes,
  HTMLAttributes,
  KeyboardEvent,
  ReactNode,
} from "react";

/**
 * Shared UI primitives for the design baseline. Views should compose these
 * instead of re-declaring sizes, radii or state colours; see tools/ui_lint.mjs.
 */

type IslandProps = HTMLAttributes<HTMLElement> & {
  as?: "section" | "div" | "aside" | "main";
  plain?: boolean;
};

export function Island({
  as: Tag = "section",
  plain = false,
  className,
  children,
  ...rest
}: IslandProps) {
  return (
    <Tag
      {...rest}
      className={["ui-island", plain ? "plain" : "", className ?? ""]
        .filter(Boolean)
        .join(" ")}
    >
      {children}
    </Tag>
  );
}

interface PanelHeaderProps {
  title: ReactNode;
  actions?: ReactNode;
  compact?: boolean;
  className?: string;
}

export function PanelHeader({
  title,
  actions,
  compact = false,
  className,
}: PanelHeaderProps) {
  return (
    <header
      className={["ui-panel-header", compact ? "compact" : "", className ?? ""]
        .filter(Boolean)
        .join(" ")}
    >
      <h3>{title}</h3>
      {actions && <div className="ui-panel-actions">{actions}</div>}
    </header>
  );
}

interface SectionHeaderProps {
  title: ReactNode;
  count?: number;
  collapsed?: boolean;
  onToggle?: () => void;
}

export function SectionHeader({
  title,
  count,
  collapsed = false,
  onToggle,
}: SectionHeaderProps) {
  const content = (
    <>
      {onToggle && (
        <span className="ui-section-caret" aria-hidden>
          {collapsed ? "▸" : "▾"}
        </span>
      )}
      <span>{title}</span>
      {typeof count === "number" && (
        <span className="ui-section-count">{count}</span>
      )}
    </>
  );
  if (!onToggle) {
    return <div className="ui-section-header">{content}</div>;
  }
  return (
    <button
      type="button"
      className="ui-section-header"
      onClick={onToggle}
      aria-expanded={!collapsed}
    >
      {content}
    </button>
  );
}

interface ListRowProps {
  title: ReactNode;
  meta?: ReactNode;
  icon?: ReactNode;
  actions?: ReactNode;
  selected?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  className?: string;
}

export function ListRow({
  title,
  meta,
  icon,
  actions,
  selected = false,
  disabled = false,
  onClick,
  className,
}: ListRowProps) {
  const interactive = Boolean(onClick) && !disabled;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    onClick?.();
  };
  return (
    <div
      className={[
        "ui-list-row",
        selected ? "selected" : "",
        disabled ? "disabled" : "",
        className ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      role={interactive ? "button" : undefined}
      tabIndex={interactive ? 0 : undefined}
      onClick={interactive ? onClick : undefined}
      onKeyDown={interactive ? onKeyDown : undefined}
    >
      {icon && <span className="ui-list-icon">{icon}</span>}
      <span className="ui-list-main">
        <span className="ui-list-title">{title}</span>
        {meta && <span className="ui-list-meta">{meta}</span>}
      </span>
      {actions && <span className="ui-row-actions">{actions}</span>}
    </div>
  );
}

export function RowActions({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={["ui-row-actions", className ?? ""].filter(Boolean).join(" ")}
    >
      {children}
    </span>
  );
}

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  size?: "sm" | "md" | "lg";
  children: ReactNode;
};

export function IconButton({
  label,
  size = "md",
  className,
  children,
  ...rest
}: IconButtonProps) {
  return (
    <button
      {...rest}
      type="button"
      className={[
        "icon-btn",
        "ui-icon-button",
        `size-${size}`,
        className ?? "",
      ]
        .filter(Boolean)
        .join(" ")}
      aria-label={label}
      title={label}
    >
      {children}
    </button>
  );
}

export function Chip({
  children,
  tone = "neutral",
  onClick,
  className,
}: {
  children: ReactNode;
  tone?: "neutral" | "info" | "warn" | "danger";
  onClick?: () => void;
  className?: string;
}) {
  return (
    <span
      className={["ui-chip", `tone-${tone}`, className ?? ""]
        .filter(Boolean)
        .join(" ")}
      onClick={onClick}
    >
      {children}
    </span>
  );
}

interface SegmentedOption {
  value: string;
  label: ReactNode;
  disabled?: boolean;
}

export function Segmented({
  options,
  value,
  onChange,
  className,
}: {
  options: SegmentedOption[];
  value: string;
  onChange: (value: string) => void;
  className?: string;
}) {
  return (
    <div
      className={["ui-segmented", className ?? ""].filter(Boolean).join(" ")}
      role="tablist"
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="tab"
          aria-selected={option.value === value}
          disabled={option.disabled}
          className={[
            "ui-segmented-item",
            option.value === value ? "active" : "",
          ]
            .filter(Boolean)
            .join(" ")}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Toolbar({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={["ui-toolbar", className ?? ""].filter(Boolean).join(" ")}>
      {children}
    </div>
  );
}

export function TableHeader({ columns }: { columns: ReactNode[] }) {
  return (
    <div className="ui-table-header">
      {columns.map((column, index) => (
        <span key={index}>{column}</span>
      ))}
    </div>
  );
}

export function EmptyState({
  title,
  description,
  action,
  icon,
  className,
}: {
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  icon?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={["ui-empty-state", className ?? ""].filter(Boolean).join(" ")}
    >
      {icon && <div className="ui-empty-icon">{icon}</div>}
      <p className="hint">{title}</p>
      {description && <p className="hint">{description}</p>}
      {action && <div className="ui-empty-action">{action}</div>}
    </div>
  );
}

export function Notice({
  children,
  tone = "info",
  className,
}: {
  children: ReactNode;
  tone?: "info" | "warn" | "danger";
  className?: string;
}) {
  return (
    <div
      className={["ui-notice", `tone-${tone}`, className ?? ""]
        .filter(Boolean)
        .join(" ")}
    >
      {children}
    </div>
  );
}

export function Tip({ children }: { children: ReactNode }) {
  return <span className="ui-tip">{children}</span>;
}

export function MenuItem({
  label,
  shortcut,
  selected = false,
  disabled = false,
  onClick,
}: {
  label: ReactNode;
  shortcut?: ReactNode;
  selected?: boolean;
  disabled?: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      className={["ui-menu-item", selected ? "selected" : ""]
        .filter(Boolean)
        .join(" ")}
      disabled={disabled}
      onClick={onClick}
    >
      <span>{label}</span>
      {shortcut && <kbd>{shortcut}</kbd>}
    </button>
  );
}

export function DialogHeader({
  title,
  onClose,
  closeLabel,
  className,
}: {
  title: ReactNode;
  onClose: () => void;
  closeLabel: string;
  className?: string;
}) {
  return (
    <header
      className={["modal-head", "ui-dialog-header", className ?? ""]
        .filter(Boolean)
        .join(" ")}
    >
      <h3>{title}</h3>
      <IconButton label={closeLabel} onClick={onClose} size="sm">
        ×
      </IconButton>
    </header>
  );
}
