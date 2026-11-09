import { useTranslation } from "react-i18next";
import {
  beginResize,
  closeWindow,
  minimizeWindow,
  toggleMaximizeWindow,
  type ResizeEdge,
} from "../window-chrome";

const EDGES: ResizeEdge[] = [
  "North",
  "South",
  "East",
  "West",
  "NorthEast",
  "NorthWest",
  "SouthEast",
  "SouthWest",
];

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

/** Invisible borders and corners that resize the frameless window. */
export function ResizeHandles() {
  return (
    <div className="resize-handles" aria-hidden>
      {EDGES.map((edge) => (
        <div
          key={edge}
          className={`resize-handle resize-${edge.toLowerCase()}`}
          onPointerDown={(event) => {
            event.preventDefault();
            void beginResize(edge);
          }}
        />
      ))}
    </div>
  );
}
