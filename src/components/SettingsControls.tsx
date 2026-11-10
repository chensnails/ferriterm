import type { ReactNode } from "react";
import { PillGroup, SettingRow, Switch, type PillOption } from "./ui";

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
        className={danger ? "link-btn danger" : undefined}
        onClick={onAction}
        disabled={disabled}
      >
        {actionLabel}
      </button>
    </SettingRow>
  );
}
