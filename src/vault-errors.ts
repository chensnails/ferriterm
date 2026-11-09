import type { TFunction } from "i18next";

/** Backend vault errors are stable English strings; translate the common ones. */
export function vaultErrorText(err: unknown, t: TFunction): string {
  const message = String(err);
  if (message.includes("master password is incorrect")) {
    return t("vault.errorWrongPassword");
  }
  const attempts = /too many attempts, retry in (\d+)s/.exec(message);
  if (attempts) {
    return t("vault.errorTooManyAttempts", { seconds: attempts[1] });
  }
  return message;
}
