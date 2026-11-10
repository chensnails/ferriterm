import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { PillGroup, SettingRow, type PillOption } from "./ui";

/**
 * A setting row whose choices are capsule buttons. The active pill is the
 * control state; the label next to it is an explicit, screen-reader friendly
 * "current value" so the state is never ambiguous.
 */
export function PillSetting({
  title,
  hint,
  options,
  value,
  onChange,
  disabled,
  wrap,
  current,
  children,
}: {
  title: string;
  hint?: ReactNode;
  options: PillOption[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  wrap?: boolean;
  current?: ReactNode;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  const active = options.find((option) => option.value === value)?.label;
  const state =
    current ??
    (typeof active === "string"
      ? t("settings.currentValue", { value: active })
      : undefined);
  return (
    <SettingRow title={title} hint={hint} current={state}>
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
