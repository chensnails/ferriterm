import "./style.css";
import { api, isTauri, type SessionKind, type SshConnectArgs } from "./api";
import { TerminalView } from "./terminal";

interface SessionEntry {
  id: string;
  kind: SessionKind;
  title: string;
  view: TerminalView;
  panel: HTMLElement;
  tab: HTMLElement;
  listItem: HTMLElement;
  closed: boolean;
}

const sessions = new Map<string, SessionEntry>();
let activeId: string | null = null;

const tabbar = document.getElementById("tabbar")!;
const terminalHost = document.getElementById("terminal-host")!;
const emptyHint = document.getElementById("empty-hint")!;
const sessionList = document.getElementById("session-list")!;
const form = document.getElementById("conn-form") as HTMLFormElement;
const formStatus = document.getElementById("form-status")!;
const localBtn = document.getElementById("local-btn") as HTMLButtonElement;
const themeToggle = document.getElementById("theme-toggle")!;

function setStatus(text: string, isError = false) {
  formStatus.textContent = text;
  formStatus.classList.toggle("error", isError);
}

function updateEmptyHint() {
  emptyHint.style.display = sessions.size === 0 ? "" : "none";
}

function activate(id: string) {
  const entry = sessions.get(id);
  if (!entry) return;
  activeId = id;
  for (const s of sessions.values()) {
    s.panel.classList.toggle("active", s.id === id);
    s.tab.classList.toggle("active", s.id === id);
    s.listItem.classList.toggle("active", s.id === id);
  }
  requestAnimationFrame(() => {
    entry.view.fitNow();
    entry.view.focus();
  });
}

async function closeSession(id: string) {
  const entry = sessions.get(id);
  if (!entry) return;
  sessions.delete(id);
  api.close(entry.kind, id).catch(() => {});
  entry.view.dispose();
  entry.panel.remove();
  entry.tab.remove();
  entry.listItem.remove();
  if (activeId === id) {
    activeId = null;
    const next = [...sessions.keys()].pop();
    if (next) activate(next);
  }
  updateEmptyHint();
}

function markClosed(id: string) {
  const entry = sessions.get(id);
  if (!entry) return;
  entry.closed = true;
  entry.tab.classList.add("closed");
  entry.listItem.classList.add("closed");
}

async function createSession(
  kind: SessionKind,
  title: string,
  connect: (id: string, cols: number, rows: number) => Promise<void>,
) {
  const id = crypto.randomUUID();

  const panel = document.createElement("div");
  panel.className = "terminal-panel";
  terminalHost.appendChild(panel);

  const tab = document.createElement("div");
  tab.className = "tab";
  const tabLabel = document.createElement("span");
  tabLabel.className = "tab-label";
  tabLabel.textContent = title;
  const closeBtn = document.createElement("button");
  closeBtn.className = "tab-close";
  closeBtn.textContent = "×";
  closeBtn.title = "关闭会话";
  closeBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    void closeSession(id);
  });
  tab.append(tabLabel, closeBtn);
  tab.addEventListener("click", () => activate(id));
  tabbar.appendChild(tab);

  const listItem = document.createElement("li");
  listItem.textContent = title;
  listItem.addEventListener("click", () => activate(id));
  sessionList.appendChild(listItem);

  const view = new TerminalView(id, kind, panel, () => markClosed(id));
  sessions.set(id, { id, kind, title, view, panel, tab, listItem, closed: false });
  updateEmptyHint();
  activate(id);

  try {
    await connect(id, view.term.cols, view.term.rows);
    setStatus("");
  } catch (err) {
    setStatus(String(err), true);
    view.term.writeln(`\r\n\x1b[31m连接失败: ${String(err)}\x1b[0m`);
    markClosed(id);
  }
}

form.addEventListener("submit", (e) => {
  e.preventDefault();
  if (!isTauri) return;
  const data = new FormData(form);
  const host = String(data.get("host") ?? "").trim();
  const username = String(data.get("username") ?? "").trim();
  const args: Omit<SshConnectArgs, "cols" | "rows"> = {
    host,
    port: Number(data.get("port")) || 22,
    username,
    password: String(data.get("password") ?? ""),
  };
  setStatus("正在连接…");
  void createSession("ssh", `${username}@${host}`, (id, cols, rows) =>
    api.sshConnect(id, { ...args, cols, rows }),
  );
});

localBtn.addEventListener("click", () => {
  if (!isTauri) return;
  void createSession("pty", "本地终端", (id, cols, rows) =>
    api.ptySpawn(id, cols, rows),
  );
});

themeToggle.addEventListener("click", () => {
  const root = document.documentElement;
  root.dataset.theme = root.dataset.theme === "dark" ? "light" : "dark";
});

if (!isTauri) {
  document.getElementById("browser-banner")!.hidden = false;
  (form.querySelector("button[type=submit]") as HTMLButtonElement).disabled = true;
  localBtn.disabled = true;
}