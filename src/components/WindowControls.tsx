import { useTranslation } from "react-i18next";
import {
  closeWindow,
  minimizeWindow,
  toggleMaximizeWindow,
} from "../window-chrome";

/**
 * Caption buttons for the frameless window. Only rendered when the frame was
 * actually removed, so they never sit above a native title bar.
 */
export function WindowControls() {
  const { t } = useTranslation();
  return (
    <div className="win-controls">
      <button
        type="button"
        className="win-btn"
        title={t("window.minimize")}
        aria-label={t("window.minimize")}
        onClick={() => void minimizeWindow()}
      >
        <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden>
          <path d="M0 5.5h10" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
      <button
        type="button"
        className="win-btn"
        title={t("window.maximize")}
        aria-label={t("window.maximize")}
        onClick={() => void toggleMaximizeWindow()}
      >
        <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden>
          <rect
            x="0.5"
            y="0.5"
            width="9"
            height="9"
            fill="none"
            stroke="currentColor"
            strokeWidth="1"
          />
        </svg>
      </button>
      <button
        type="button"
        className="win-btn win-btn-close"
        title={t("window.close")}
        aria-label={t("window.close")}
        onClick={() => void closeWindow()}
      >
        <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden>
          <path
            d="M0.5 0.5l9 9M9.5 0.5l-9 9"
            stroke="currentColor"
            strokeWidth="1"
          />
        </svg>
      </button>
    </div>
  );
}
