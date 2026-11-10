import {
  Fragment,
  createContext,
  useContext,
  useId,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { beginWindowDrag, zoomWindowOnDoubleClick } from "../window-chrome";
import { CloseIcon } from "./icons";

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
  /** Makes the bar's empty space a second way to move the window. */
  dragRegion?: boolean;
}

export function PanelHeader({
  title,
  actions,
  compact = false,
  className,
  dragRegion = false,
}: PanelHeaderProps) {
  return (
    <header
      className={["ui-panel-header", compact ? "compact" : "", className ?? ""]
        .filter(Boolean)
        .join(" ")}
      onMouseDown={dragRegion ? beginWindowDrag : undefined}
      onDoubleClick={dragRegion ? zoomWindowOnDoubleClick : undefined}
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

/** One icon slot in the 40px activity rail / tool strip. */
export function RailButton({
  label,
  active = false,
  pressed,
  badge = false,
  badgeLabel,
  disabled = false,
  onClick,
  children,
}: {
  label: string;
  active?: boolean;
  pressed?: boolean;
  badge?: boolean;
  badgeLabel?: string;
  /** Reserved slots stay visible but cannot be activated yet. */
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={["ui-rail-button", active ? "active" : ""]
        .filter(Boolean)
        .join(" ")}
      aria-label={badge && badgeLabel ? `${label} — ${badgeLabel}` : label}
      title={badge && badgeLabel ? `${label} — ${badgeLabel}` : label}
      // Toggle slots expose aria-pressed; module slots expose aria-current.
      aria-current={pressed === undefined && active ? "page" : undefined}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
      {badge && <span className="ui-rail-badge" aria-hidden />}
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

export interface PillOption {
  value: string;
  label: ReactNode;
  disabled?: boolean;
  /** Shown as a title tooltip so long algorithm names stay readable. */
  hint?: string;
  /** Optional optgroup-style heading, rendered before the first of a run. */
  group?: string;
}

type PillGroupProps =
  | {
      options: PillOption[];
      value: string;
      onChange: (value: string) => void;
      multiple?: false;
      disabled?: boolean;
      wrap?: boolean;
      label: string;
    }
  | {
      options: PillOption[];
      value: string[];
      onChange: (value: string[]) => void;
      multiple: true;
      disabled?: boolean;
      wrap?: boolean;
      label: string;
    };

/**
 * Clickable capsule buttons replacing settings dropdowns. Single-select uses
 * radio semantics, multi-select uses checkbox semantics; both honour per-option
 * and whole-group disabled states and expose the current value to AT.
 */
export function PillGroup(props: PillGroupProps) {
  const { options, disabled, wrap, label } = props;
  const multiple = props.multiple === true;
  const selected: readonly string[] = multiple ? props.value : [props.value];
  let lastGroup: string | undefined;

  return (
    <div
      className={["ui-pill-group", wrap ? "wrap" : ""].filter(Boolean).join(" ")}
      role={multiple ? "group" : "radiogroup"}
      aria-label={label}
      aria-disabled={disabled || undefined}
    >
      {options.map((option) => {
        const heading = option.group !== lastGroup ? option.group : undefined;
        lastGroup = option.group;
        const active = selected.includes(option.value);
        const isDisabled = disabled || option.disabled;
        return (
          <Fragment key={option.value}>
            {heading && <span className="ui-pill-heading">{heading}</span>}
            <button
              type="button"
              className={["ui-pill", active ? "active" : ""]
                .filter(Boolean)
                .join(" ")}
              role={multiple ? "checkbox" : "radio"}
              aria-checked={active}
              disabled={isDisabled}
              title={option.hint}
              onClick={() => {
                if (multiple) {
                  const next = active
                    ? props.value.filter((item) => item !== option.value)
                    : [...props.value, option.value];
                  props.onChange(next);
                } else if (!active) {
                  props.onChange(option.value);
                }
              }}
            >
              {option.label}
            </button>
          </Fragment>
        );
      })}
    </div>
  );
}

/** Consistent title / description / current-state / control row. */
/**
 * Settings search query, shared by every setting row so the sidebar search can
 * filter rows without a per-row registry. Empty string means "no filter".
 */
export const SettingsSearchContext = createContext("");

/** Flattens a ReactNode into plain text so rows can be matched by keyword. */
export function toSearchText(node: ReactNode): string {
  if (node === null || node === undefined || typeof node === "boolean") {
    return "";
  }
  if (typeof node === "string" || typeof node === "number") {
    return String(node);
  }
  if (Array.isArray(node)) {
    return node.map(toSearchText).join(" ");
  }
  if (typeof node === "object" && "props" in node) {
    return toSearchText((node as { props?: { children?: ReactNode } }).props?.children);
  }
  return "";
}

/** A titled settings row: copy on the left, the control on the right. */
export function SettingRow({
  title,
  hint,
  current,
  children,
  stack,
  className,
}: {
  title: ReactNode;
  hint?: ReactNode;
  current?: ReactNode;
  children: ReactNode;
  /** Puts the control under the copy — for long option lists. */
  stack?: boolean;
  className?: string;
}) {
  const query = useContext(SettingsSearchContext).trim().toLowerCase();
  if (
    query &&
    !`${toSearchText(title)} ${toSearchText(hint)}`.toLowerCase().includes(query)
  ) {
    return null;
  }
  return (
    <div
      className={["ui-setting-row", stack ? "stack" : "", className ?? ""]
        .filter(Boolean)
        .join(" ")}
    >
      <div className="ui-setting-copy">
        <span className="ui-setting-title">{title}</span>
        {hint && <span className="ui-setting-hint">{hint}</span>}
      </div>
      <div className="ui-setting-control">
        {current !== undefined && (
          <span className="ui-setting-current">{current}</span>
        )}
        {children}
      </div>
    </div>
  );
}

/** An on/off control that keeps `role="switch"` semantics and keyboard access. */
export function Switch({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={["ui-switch", checked ? "on" : ""].filter(Boolean).join(" ")}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className="ui-switch-thumb" />
    </button>
  );
}

/**
 * A titled block of related settings. Rendered as a flat section label instead
 * of a bordered fieldset so the pane reads as one continuous list; an optional
 * reset action sits on the label row.
 */
export function SettingGroup({
  title,
  hint,
  onReset,
  resetLabel,
  children,
}: {
  title: ReactNode;
  hint?: ReactNode;
  onReset?: () => void;
  resetLabel?: string;
  children: ReactNode;
}) {
  const titleId = useId();
  const query = useContext(SettingsSearchContext);
  const trimmed = query.trim().toLowerCase();
  // A matching group title keeps the whole block visible, so searching for
  // "highlights" still shows the related rows.
  const keepAll =
    trimmed.length > 0 && toSearchText(title).toLowerCase().includes(trimmed);
  return (
    <section className="ui-setting-group" aria-labelledby={titleId}>
      <div className="ui-setting-group-head">
        <h3 className="ui-setting-group-title" id={titleId}>
          {title}
        </h3>
        {onReset && resetLabel && (
          <button
            type="button"
            className="link-btn ui-setting-group-reset"
            onClick={onReset}
          >
            {resetLabel}
          </button>
        )}
      </div>
      {hint && <p className="ui-setting-group-hint">{hint}</p>}
      <div className="ui-setting-group-body">
        <SettingsSearchContext.Provider value={keepAll ? "" : query}>
          {children}
        </SettingsSearchContext.Provider>
      </div>
    </section>
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
  subtitle,
  icon,
  onClose,
  closeLabel,
  className,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  icon?: ReactNode;
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
      <div className="ui-dialog-heading">
        {icon && <span className="ui-dialog-icon">{icon}</span>}
        <div className="ui-dialog-copy">
          <h3>{title}</h3>
          {subtitle && <p className="ui-dialog-subtitle">{subtitle}</p>}
        </div>
      </div>
      <IconButton label={closeLabel} onClick={onClose} size="sm">
        <CloseIcon size={13} />
      </IconButton>
    </header>
  );
}
