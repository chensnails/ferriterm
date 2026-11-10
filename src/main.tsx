import { createRoot } from "react-dom/client";
import "./i18n";
import "./style.css";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";

// Keep the window alive on async failures the boundary cannot see (rejected
// promises from imperative handlers, uncaught throws in timers). Surfacing
// them here is the single place a future logger or toast can subscribe.
window.addEventListener("error", (event) => {
  console.error("Ferriterm window error:", event.error ?? event.message);
});
window.addEventListener("unhandledrejection", (event) => {
  console.error("Ferriterm unhandled rejection:", event.reason);
});

// No StrictMode: TerminalView owns an imperative xterm instance and a one-shot
// connect; the dev double-mount would dispose the live view and drop the session.
createRoot(document.getElementById("root")!).render(
  <ErrorBoundary>
    <App />
  </ErrorBoundary>,
);