import type { ReactNode } from "react";
import {
  PillGroup,
  Segmented,
  SettingRow,
  Switch,
  type PillOption,
} from "./ui";

/**
 * A row whose on/off state is a switch. Switches carry the state themselves, so
 * no separate "current value" label is duplicated next to the control.
 */
export function ToggleSetting({
  title,
  hint,
  checked,
  onChange,
  disabled,
}: {
  title: string;
  hint?: ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <SettingRow title={title} hint={hint}>
      <Switch
        checked={checked}
        onChange={onChange}
        disabled={disabled}
        label={title}
      />
    </SettingRow>
  );
}

/**
 * A setting row whose choices are capsule buttons. The highlighted pill is the
 * current value, so no extra "current: …" label is duplicated next to it.
 * `stack` puts the pills under the copy for long option lists.
 */
export function PillSetting({
  title,
  hint,
  options,
  value,
  onChange,
  disabled,
  wrap,
  stack,
  children,
}: {
  title: string;
  hint?: ReactNode;
  options: PillOption[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  wrap?: boolean;
  stack?: boolean;
  children?: ReactNode;
}) {
  return (
    <SettingRow title={title} hint={hint} stack={stack}>
      {children}
      <PillGroup
        options={options}
        value={value}
        onChange={onChange}
        disabled={disabled}
        wrap={wrap}
        label={title}
      />
    </SettingRow>
  );
}

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
  /** Consecutive options sharing a group render as one <optgroup>. */
  group?: string;
}

/**
 * A row with a small, mutually exclusive set of choices. The connected strip
 * separates "pick one of these modes" from the capsule row used for policy
 * presets, so not every short list looks like the same control.
 */
export function SegmentedSetting({
  title,
  hint,
  options,
  value,
  onChange,
  disabled,
}: {
  title: string;
  hint?: ReactNode;
  options: { value: string; label: ReactNode; disabled?: boolean }[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <SettingRow title={title} hint={hint}>
      <Segmented
        options={options}
        value={value}
        onChange={onChange}
        label={title}
        semantics="radio"
        disabled={disabled}
      />
    </SettingRow>
  );
}

/**
 * A row whose value comes from a long list. Native <select> keeps keyboard
 * navigation, type-ahead and OS dropdown behaviour, and stays compact where a
 * pill row would wrap into several lines.
 */
export function SelectSetting({
  title,
  hint,
  value,
  onChange,
  options,
  disabled,
  label,
  wide,
}: {
  title: string;
  hint?: ReactNode;
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  disabled?: boolean;
  label?: string;
  /** Lets the control use the full 280px column instead of the short width. */
  wide?: boolean;
}) {
  const groups: { name?: string; items: SelectOption[] }[] = [];
  for (const option of options) {
    const last = groups[groups.length - 1];
    if (last && last.name === option.group) last.items.push(option);
    else groups.push({ name: option.group, items: [option] });
  }

  return (
    <SettingRow title={title} hint={hint}>
      <select
        className={["ui-select", wide ? "wide" : ""].filter(Boolean).join(" ")}
        value={value}
        aria-label={label ?? title}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
      >
        {groups.map((group, index) =>
          group.name === undefined ? (
            group.items.map((option) => (
              <option
                key={option.value}
                value={option.value}
                disabled={option.disabled}
              >
                {option.label}
              </option>
            ))
          ) : (
            <optgroup key={`${group.name}-${index}`} label={group.name}>
              {group.items.map((option) => (
                <option
                  key={option.value}
                  value={option.value}
                  disabled={option.disabled}
                >
                  {option.label}
                </option>
              ))}
            </optgroup>
          ),
        )}
      </select>
    </SettingRow>
  );
}

/** A row with a real action button (clear cache, reset, save…). */
export function ActionSetting({
  title,
  hint,
  current,
  actionLabel,
  onAction,
  disabled,
  danger,
}: {
  title: string;
  hint?: ReactNode;
  current?: ReactNode;
  actionLabel: string;
  onAction: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <SettingRow title={title} hint={hint} current={current}>
      <button
        type="button"
        className={danger ? "ui-setting-action danger" : "ui-setting-action"}
        onClick={onAction}
        disabled={disabled}
      >
        {actionLabel}
      </button>
    </SettingRow>
  );
}
