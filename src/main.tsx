import { createRoot } from "react-dom/client";
import "./i18n";
import "./style.css";
import App from "./App";

// No StrictMode: TerminalView owns an imperative xterm instance and a one-shot
// connect; the dev double-mount would dispose the live view and drop the session.
createRoot(document.getElementById("root")!).render(<App />);