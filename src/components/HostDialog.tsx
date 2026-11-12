import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  DEFAULT_PROTOCOL_OPTIONS,
  isTauri,
  type HostEntry,
  type HostInput,
  type HostProtocol,
  type KeyCandidate,
  type PasswordCredential,
  type ProtocolOptions,
  type SerialFlowControl,
  type SerialParity,
  type SerialPortInfo,
  type SshKey,
} from "../api";
import { HOST_ICONS, HostIcon } from "../host-icons";
import { useAppStore } from "../store";
import { Modal } from "./Modal";
import { TagChip } from "./TagChip";

interface HostDialogProps {
  host?: HostEntry;
  parentId: string | null;
  isFolder: boolean;
  onClose: () => void;
}

const BAUD_RATES = [
  4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
];

const ENCODINGS: { value: string; label: string }[] = [
  { value: "utf-8", label: "UTF-8" },
  { value: "gbk", label: "GBK" },
  { value: "gb18030", label: "GB18030" },
  { value: "big5", label: "Big5" },
  { value: "shift_jis", label: "Shift_JIS" },
  { value: "euc-jp", label: "EUC-JP" },
  { value: "euc-kr", label: "EUC-KR" },
  { value: "windows-1252", label: "Windows-1252" },
];

const PROTOCOLS: HostProtocol[] = ["ssh", "sftp", "telnet", "serial", "local"];

const DEFAULT_PORTS: Record<HostProtocol, number> = {
  ssh: 22,
  sftp: 22,
  telnet: 23,
  serial: 0,
  local: 0,
};

const PROTOCOL_LABEL: Record<HostProtocol, string> = {
  ssh: "hostDialog.protocolSsh",
  sftp: "hostDialog.protocolSftp",
  telnet: "hostDialog.protocolTelnet",
  serial: "hostDialog.protocolSerial",
  local: "hostDialog.protocolLocal",
};

const ICON_GROUPS = ["system", "network", "device"] as const;

const GROUP_LABEL: Record<(typeof ICON_GROUPS)[number], string> = {
  system: "hostDialog.iconSystems",
  network: "hostDialog.iconNetwork",
  device: "hostDialog.iconDevices",
};

export function HostDialog({
  host,
  parentId,
  isFolder,
  onClose,
}: HostDialogProps) {
  const { t } = useTranslation();
  const tags = useAppStore((s) => s.tags);
  const hosts = useAppStore((s) => s.hosts);
  const pluginScan = useAppStore((s) => s.pluginScan);
  const telnetReady = !!pluginScan?.plugins.some(
    (plugin) => plugin.kind === "telnet",
  );
  const createHost = useAppStore((s) => s.createHost);
  const updateHost = useAppStore((s) => s.updateHost);
  const loadHosts = useAppStore((s) => s.loadHosts);

  // Retries keep one id so a failed save can never leave a duplicate host.
  const [draftId] = useState(() => host?.id ?? crypto.randomUUID());
  const [name, setName] = useState(host?.name ?? "");
  const [protocol, setProtocol] = useState<HostProtocol>(
    host?.protocol ?? "ssh",
  );
  const [address, setAddress] = useState(host?.host ?? "");
  const [port, setPort] = useState(
    String(host?.port || DEFAULT_PORTS[host?.protocol ?? "ssh"]),
  );
  const [username, setUsername] = useState(host?.username ?? "");
  const [authMethod, setAuthMethod] = useState(host?.authMethod ?? "password");
  const [keyPath, setKeyPath] = useState(host?.keyPath ?? "");
  const [keyId, setKeyId] = useState(host?.keyId ?? "");
  const [passwordId, setPasswordId] = useState(host?.passwordId ?? "");
  const [passwordSource, setPasswordSource] = useState<"direct" | "credential">(
    host?.passwordId ? "credential" : "direct",
  );
  const [directPassword, setDirectPassword] = useState("");
  const [keySource, setKeySource] = useState<"file" | "managed">(
    host?.keyId ? "managed" : "file",
  );
  const [keyPassphrase, setKeyPassphrase] = useState("");
  const [keyCandidates, setKeyCandidates] = useState<KeyCandidate[]>([]);
  const [scanning, setScanning] = useState(false);
  const [keys, setKeys] = useState<SshKey[]>([]);
  const [passwords, setPasswords] = useState<PasswordCredential[]>([]);
  const [jumpHostId, setJumpHostId] = useState(host?.jumpHostId ?? "");
  const [x11, setX11] = useState(host?.x11 ?? false);
  const [options, setOptions] = useState<ProtocolOptions>({
    ...DEFAULT_PROTOCOL_OPTIONS,
    ...(host?.protocolOptions ?? {}),
  });
  const [icon, setIcon] = useState(host?.icon ?? "");
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [portsLoading, setPortsLoading] = useState(false);
  const [production, setProduction] = useState(host?.production ?? false);
  const [notes, setNotes] = useState(host?.notes ?? "");
  const [tagIds, setTagIds] = useState<string[]>(
    host?.tags.map((tag) => tag.id) ?? [],
  );
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const createdRef = useRef(false);

  const sshLike = protocol === "ssh" || protocol === "sftp";
  const networkProtocol = protocol !== "serial" && protocol !== "local";

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    void api
      .sshKeyList()
      .then((list) => {
        if (!cancelled) setKeys(list);
      })
      .catch(() => {});
    void api
      .passwordCredentialList()
      .then((list) => {
        if (!cancelled) setPasswords(list);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const refreshPorts = useCallback(async () => {
    if (!isTauri) return;
    setPortsLoading(true);
    try {
      const list = await api.serialListPorts();
      setPorts(list);
      setOptions((current) => ({
        ...current,
        serialPath: current.serialPath || list[0]?.path || "",
      }));
    } catch {
      // enumeration is best-effort; the port path stays editable by hand
    } finally {
      setPortsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (protocol === "serial" && isTauri) void refreshPorts();
  }, [protocol, refreshPorts]);

  const scanKeys = useCallback(async () => {
    if (!isTauri) return;
    setScanning(true);
    try {
      setKeyCandidates(await api.sshKeyScan());
    } catch {
      setKeyCandidates([]);
    } finally {
      setScanning(false);
    }
  }, []);

  const title = host
    ? isFolder
      ? t("hostDialog.editFolderTitle")
      : t("hostDialog.editHostTitle")
    : isFolder
      ? t("hostDialog.newFolderTitle")
      : t("hostDialog.newHostTitle");

  const toggleTag = (id: string) =>
    setTagIds((ids) =>
      ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id],
    );

  const updateOption = <K extends keyof ProtocolOptions>(
    key: K,
    value: ProtocolOptions[K],
  ) => setOptions((current) => ({ ...current, [key]: value }));

  const changeProtocol = (next: HostProtocol) => {
    const current = Number(port);
    if (!current || current === DEFAULT_PORTS[protocol]) {
      setPort(String(DEFAULT_PORTS[next]));
    }
    setProtocol(next);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    if (!isFolder) {
      if (networkProtocol && !address.trim()) {
        setError(t("hostDialog.hostRequired"));
        return;
      }
      if (protocol === "serial" && !options.serialPath.trim()) {
        setError(t("hostDialog.serialPathRequired"));
        return;
      }
    }
    setSaving(true);
    const typedPassword =
      !isFolder &&
      sshLike &&
      authMethod === "password" &&
      passwordSource === "direct"
        ? directPassword
        : "";
    const typedPassphrase =
      !isFolder && sshLike && authMethod === "key" && keySource === "file"
        ? keyPassphrase
        : "";
    const input: HostInput = {
      id: draftId,
      name: name.trim(),
      host: isFolder || !networkProtocol ? "" : address.trim(),
      port:
        isFolder || !networkProtocol
          ? 22
          : Number(port) || DEFAULT_PORTS[protocol],
      username: isFolder || !sshLike ? "" : username.trim(),
      authMethod: isFolder || !sshLike ? "password" : authMethod,
      keyPath:
        isFolder || !sshLike || authMethod !== "key" || keySource !== "file"
          ? ""
          : keyPath.trim(),
      keyId:
        isFolder || !sshLike || authMethod !== "key" || keySource !== "managed"
          ? ""
          : keyId,
      passwordId:
        isFolder || !sshLike || authMethod !== "password"
          ? ""
          : passwordSource === "credential"
            ? passwordId
            : typedPassword
              ? ""
              : (host?.passwordId ?? ""),
      jumpHostId: isFolder || !sshLike ? "" : jumpHostId,
      x11: isFolder || protocol !== "ssh" ? false : x11,
      parentId: host ? host.parentId : parentId,
      isFolder,
      production: isFolder ? false : production,
      protocol: isFolder ? "ssh" : protocol,
      protocolOptions: isFolder
        ? DEFAULT_PROTOCOL_OPTIONS
        : {
            ...options,
            serialPath: options.serialPath.trim(),
            shell: options.shell.trim(),
            cwd: options.cwd.trim(),
          },
      icon: isFolder ? "" : icon,
      notes: isFolder ? "" : notes.trim(),
      tagIds: isFolder ? [] : tagIds,
    };
    try {
      if (host || createdRef.current) {
        await updateHost(input);
      } else {
        await createHost(input);
        createdRef.current = true;
      }
      if (typedPassword && isTauri) {
        // Typed passwords become ordinary named credentials, so the vault
        // manager can edit or reuse them and the connect dialog stays out of
        // the way.
        await api.passwordCredentialSave({
          id: crypto.randomUUID(),
          name:
            name.trim() ||
            [username.trim(), address.trim()].filter(Boolean).join("@") ||
            draftId,
          username: username.trim(),
          secret: typedPassword,
          hostId: draftId,
        });
        await loadHosts();
      }
      if (typedPassphrase && isTauri) {
        await api.saveHostCredential(draftId, null, typedPassphrase);
      }
      setDirectPassword("");
      setKeyPassphrase("");
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="submit"
            form="host-dialog-form"
            className="primary"
            data-testid="host-save"
            disabled={saving}
          >
            {saving ? t("hostDialog.saving") : t("hostDialog.save")}
          </button>
        </>
      }
    >
      <form id="host-dialog-form" className="form-grid" onSubmit={submit}>
        <label className="field">
          <span>{t("hostDialog.name")}</span>
          <div className="field-inline">
            <span className="name-icon-preview">
              <HostIcon icon={icon} protocol={protocol} size={18} />
            </span>
            <input
              autoFocus
              data-testid="host-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("hostDialog.namePlaceholder")}
              required
            />
          </div>
        </label>

        {isFolder ? (
          <p className="hint">{t("hostDialog.folderHint")}</p>
        ) : (
          <>
            <div className="protocol-tabs" role="tablist">
              {PROTOCOLS.map((value) => (
                <button
                  key={value}
                  type="button"
                  role="tab"
                  aria-selected={protocol === value}
                  className={protocol === value ? "active" : ""}
                  onClick={() => changeProtocol(value)}
                >
                  <HostIcon protocol={value} size={14} />
                  {t(PROTOCOL_LABEL[value])}
                </button>
              ))}
            </div>

            {networkProtocol && (
              <>
                <label className="field">
                  <span>{t("hostDialog.host")}</span>
                  <input
                    data-testid="host-address"
                    value={address}
                    onChange={(e) => setAddress(e.target.value)}
                    placeholder={t("connect.hostPlaceholder")}
                  />
                </label>
                <div className="field-row">
                  <label className="field">
                    <span>{t("hostDialog.port")}</span>
                    <input
                      type="number"
                      min={1}
                      max={65535}
                      data-testid="host-port"
                      value={port}
                      onChange={(e) => setPort(e.target.value)}
                    />
                  </label>
                  {sshLike && (
                    <label className="field grow">
                      <span>{t("hostDialog.username")}</span>
                      <input
                        data-testid="host-username"
                        value={username}
                        onChange={(e) => setUsername(e.target.value)}
                        placeholder={t("connect.usernamePlaceholder")}
                      />
                    </label>
                  )}
                </div>
                {protocol === "telnet" && (
                  <>
                    <p className="hint">{t("hostDialog.telnetHint")}</p>
                    {!telnetReady && (
                      <p className="hint">
                        {t("hostDialog.telnetNeedsPlugin")}
                      </p>
                    )}
                  </>
                )}
                {protocol === "sftp" && (
                  <p className="hint">{t("hostDialog.sftpHint")}</p>
                )}
              </>
            )}

            {sshLike && (
              <>
                <label className="field">
                  <span>{t("hostDialog.authMethod")}</span>
                  <select
                    value={authMethod}
                    onChange={(e) => setAuthMethod(e.target.value)}
                  >
                    <option value="password">
                      {t("hostDialog.authPassword")}
                    </option>
                    <option value="key">{t("hostDialog.authKey")}</option>
                    <option value="agent">{t("hostDialog.authAgent")}</option>
                    <option value="keyboard-interactive">
                      {t("hostDialog.authKeyboardInteractive")}
                    </option>
                  </select>
                </label>

                {authMethod === "agent" && (
                  <p className="hint">{t("hostDialog.agentHint")}</p>
                )}

                {authMethod === "password" && (
                  <>
                    <label className="field">
                      <span>{t("hostDialog.passwordSource")}</span>
                      <select
                        value={passwordSource}
                        onChange={(e) =>
                          setPasswordSource(
                            e.target.value as "direct" | "credential",
                          )
                        }
                      >
                        <option value="direct">
                          {t("hostDialog.sourceDirect")}
                        </option>
                        <option value="credential">
                          {t("hostDialog.sourceCredential")}
                        </option>
                      </select>
                    </label>
                    {passwordSource === "direct" ? (
                      <label className="field">
                        <span>{t("hostDialog.directPassword")}</span>
                        <input
                          type="password"
                          autoComplete="new-password"
                          data-testid="host-password"
                          value={directPassword}
                          onChange={(e) => setDirectPassword(e.target.value)}
                          placeholder={t(
                            "hostDialog.directPasswordPlaceholder",
                          )}
                        />
                        <span className="hint">
                          {passwordId
                            ? t("hostDialog.directPasswordKeepHint")
                            : t("hostDialog.directPasswordHint")}
                        </span>
                      </label>
                    ) : (
                      <label className="field">
                        <span>{t("hostDialog.passwordPick")}</span>
                        <select
                          value={passwordId}
                          onChange={(e) => setPasswordId(e.target.value)}
                        >
                          <option value="">
                            {t("hostDialog.passwordPickEmpty")}
                          </option>
                          {passwords.map((credential) => (
                            <option key={credential.id} value={credential.id}>
                              {credential.name}
                              {credential.username
                                ? ` · ${credential.username}`
                                : ""}
                            </option>
                          ))}
                        </select>
                        <span className="hint">
                          {passwords.length === 0
                            ? t("hostDialog.passwordsEmpty")
                            : t("hostDialog.passwordCredentialHint")}
                        </span>
                      </label>
                    )}
                  </>
                )}

                {authMethod === "keyboard-interactive" && (
                  <p className="hint">
                    {t("hostDialog.keyboardInteractiveHint")}
                  </p>
                )}

                {authMethod === "key" && (
                  <>
                    <label className="field">
                      <span>{t("hostDialog.keySource")}</span>
                      <select
                        value={keySource}
                        onChange={(e) =>
                          setKeySource(e.target.value as "file" | "managed")
                        }
                      >
                        <option value="file">
                          {t("hostDialog.sourceFile")}
                        </option>
                        <option value="managed">
                          {t("hostDialog.sourceManaged")}
                        </option>
                      </select>
                    </label>
                    {keySource === "file" ? (
                      <>
                        <label className="field">
                          <span>{t("hostDialog.keyPath")}</span>
                          <div className="field-inline">
                            <input
                              value={keyPath}
                              onChange={(e) => setKeyPath(e.target.value)}
                              placeholder="~/.ssh/id_ed25519"
                            />
                            <button
                              type="button"
                              onClick={() => void scanKeys()}
                              disabled={scanning || !isTauri}
                            >
                              {scanning
                                ? t("hostDialog.scanning")
                                : t("hostDialog.keyScan")}
                            </button>
                          </div>
                          {keyCandidates.length > 0 && (
                            <select
                              value={
                                keyCandidates.some(
                                  (candidate) => candidate.path === keyPath,
                                )
                                  ? keyPath
                                  : ""
                              }
                              onChange={(e) => setKeyPath(e.target.value)}
                            >
                              <option value="">
                                {t("hostDialog.keyCandidatePick")}
                              </option>
                              {keyCandidates.map((candidate) => (
                                <option
                                  key={candidate.path}
                                  value={candidate.path}
                                >
                                  {candidate.name} · {candidate.algorithm} ·{" "}
                                  {candidate.fingerprint}
                                </option>
                              ))}
                            </select>
                          )}
                          {isTauri && keyCandidates.length === 0 && (
                            <span className="hint">
                              {t("hostDialog.keyFileHint")}
                            </span>
                          )}
                        </label>
                        <label className="field">
                          <span>{t("hostDialog.keyPassphrase")}</span>
                          <input
                            type="password"
                            autoComplete="new-password"
                            value={keyPassphrase}
                            onChange={(e) => setKeyPassphrase(e.target.value)}
                            placeholder={t(
                              "hostDialog.keyPassphrasePlaceholder",
                            )}
                          />
                          <span className="hint">
                            {t("hostDialog.keyPassphraseHint")}
                          </span>
                        </label>
                      </>
                    ) : (
                      <label className="field">
                        <span>{t("hostDialog.keyPick")}</span>
                        <select
                          value={keyId}
                          onChange={(e) => setKeyId(e.target.value)}
                        >
                          <option value="">
                            {t("hostDialog.keyPickEmpty")}
                          </option>
                          {keys.map((key) => (
                            <option key={key.id} value={key.id}>
                              {key.name} · {key.fingerprint}
                            </option>
                          ))}
                        </select>
                        <span className="hint">
                          {keys.length === 0
                            ? t("hostDialog.keysEmpty")
                            : t("hostDialog.keyManagedHint")}
                        </span>
                      </label>
                    )}
                  </>
                )}

                <label className="field">
                  <span>{t("hostDialog.jumpHost")}</span>
                  <select
                    value={jumpHostId}
                    onChange={(e) => setJumpHostId(e.target.value)}
                  >
                    <option value="">{t("hostDialog.jumpNone")}</option>
                    {hosts
                      .filter(
                        (item) =>
                          !item.isFolder &&
                          item.id !== host?.id &&
                          (item.protocol === "ssh" || item.protocol === "sftp"),
                      )
                      .map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.name || `${item.username}@${item.host}`}
                        </option>
                      ))}
                  </select>
                  <span className="hint">{t("hostDialog.jumpHint")}</span>
                </label>

                <label className="field">
                  <span>{t("hostDialog.encoding")}</span>
                  <select
                    value={options.encoding}
                    onChange={(e) => updateOption("encoding", e.target.value)}
                  >
                    {ENCODINGS.map((item) => (
                      <option key={item.value} value={item.value}>
                        {item.label}
                      </option>
                    ))}
                  </select>
                  <span className="hint">{t("hostDialog.encodingHint")}</span>
                </label>
              </>
            )}

            {protocol === "ssh" && (
              <>
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={x11}
                    onChange={(e) => setX11(e.target.checked)}
                  />
                  <span>{t("hostDialog.x11")}</span>
                </label>
                <p className="hint">{t("hostDialog.x11Hint")}</p>
              </>
            )}

            {protocol === "serial" && (
              <>
                <p className="hint">{t("serial.needsPlugin")}</p>
                <label className="field">
                  <span>{t("serial.port")}</span>
                  <div className="field-inline">
                    <input
                      list="host-serial-ports"
                      value={options.serialPath}
                      placeholder="/dev/ttyUSB0"
                      onChange={(e) =>
                        updateOption("serialPath", e.target.value)
                      }
                    />
                    <datalist id="host-serial-ports">
                      {ports.map((item) => (
                        <option key={item.path} value={item.path}>
                          {item.name}
                        </option>
                      ))}
                    </datalist>
                    <button
                      type="button"
                      onClick={() => void refreshPorts()}
                      disabled={portsLoading || !isTauri}
                    >
                      {t("serial.refreshPorts")}
                    </button>
                  </div>
                  {isTauri && !portsLoading && ports.length === 0 && (
                    <span className="hint">{t("serial.noPorts")}</span>
                  )}
                </label>
                <label className="field">
                  <span>{t("serial.baudRate")}</span>
                  <div className="field-inline">
                    <input
                      type="number"
                      min={50}
                      max={4000000}
                      list="host-baud-rates"
                      value={options.baudRate}
                      onChange={(e) =>
                        updateOption("baudRate", Number(e.target.value))
                      }
                    />
                    <datalist id="host-baud-rates">
                      {BAUD_RATES.map((rate) => (
                        <option key={rate} value={rate} />
                      ))}
                    </datalist>
                  </div>
                </label>
                <div className="serial-grid">
                  <label className="field">
                    <span>{t("serial.dataBits")}</span>
                    <select
                      value={options.dataBits}
                      onChange={(e) =>
                        updateOption("dataBits", Number(e.target.value))
                      }
                    >
                      {[5, 6, 7, 8].map((bits) => (
                        <option key={bits} value={bits}>
                          {bits}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>{t("serial.stopBits")}</span>
                    <select
                      value={options.stopBits}
                      onChange={(e) =>
                        updateOption("stopBits", Number(e.target.value))
                      }
                    >
                      <option value={1}>1</option>
                      <option value={2}>2</option>
                    </select>
                  </label>
                  <label className="field">
                    <span>{t("serial.parity")}</span>
                    <select
                      value={options.parity}
                      onChange={(e) =>
                        updateOption("parity", e.target.value as SerialParity)
                      }
                    >
                      <option value="none">{t("serial.parityNone")}</option>
                      <option value="odd">{t("serial.parityOdd")}</option>
                      <option value="even">{t("serial.parityEven")}</option>
                    </select>
                  </label>
                  <label className="field">
                    <span>{t("serial.flowControl")}</span>
                    <select
                      value={options.flowControl}
                      onChange={(e) =>
                        updateOption(
                          "flowControl",
                          e.target.value as SerialFlowControl,
                        )
                      }
                    >
                      <option value="none">{t("serial.flowNone")}</option>
                      <option value="rtscts">{t("serial.flowRtsCts")}</option>
                      <option value="xonxoff">{t("serial.flowXonXoff")}</option>
                    </select>
                  </label>
                </div>
              </>
            )}

            {protocol === "local" && (
              <>
                <label className="field">
                  <span>{t("hostDialog.localShell")}</span>
                  <input
                    value={options.shell}
                    onChange={(e) => updateOption("shell", e.target.value)}
                    placeholder={t("hostDialog.localShellPlaceholder")}
                  />
                  <span className="hint">{t("hostDialog.localShellHint")}</span>
                </label>
                <label className="field">
                  <span>{t("hostDialog.localCwd")}</span>
                  <input
                    value={options.cwd}
                    onChange={(e) => updateOption("cwd", e.target.value)}
                    placeholder={t("hostDialog.localCwdPlaceholder")}
                  />
                </label>
              </>
            )}

            <fieldset className="icon-picker">
              <legend>{t("hostDialog.icon")}</legend>
              <button
                type="button"
                className={`icon-auto${icon ? "" : " active"}`}
                onClick={() => setIcon("")}
                aria-pressed={!icon}
              >
                <HostIcon protocol={protocol} size={18} />
                <span>{t("hostDialog.iconAuto")}</span>
              </button>
              {ICON_GROUPS.map((group) => (
                <div key={group} className="icon-group">
                  <span className="icon-group-label">
                    {t(GROUP_LABEL[group])}
                  </span>
                  <div className="icon-options">
                    {HOST_ICONS.filter((def) => def.group === group).map(
                      (def) => (
                        <button
                          key={def.id}
                          type="button"
                          className={`icon-option${icon === def.id ? " active" : ""}`}
                          onClick={() => setIcon(def.id)}
                          title={def.name}
                          aria-label={def.name}
                          aria-pressed={icon === def.id}
                        >
                          <HostIcon
                            icon={def.id}
                            protocol={protocol}
                            size={18}
                          />
                        </button>
                      ),
                    )}
                  </div>
                </div>
              ))}
            </fieldset>

            <label className="field">
              <span>{t("hostDialog.notes")}</span>
              <textarea
                rows={3}
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                placeholder={t("hostDialog.notesPlaceholder")}
              />
            </label>

            <fieldset className="tag-picker">
              <legend>{t("hostDialog.tags")}</legend>
              {tags.length === 0 ? (
                <p className="hint">{t("hostDialog.noTags")}</p>
              ) : (
                <div className="tag-options">
                  {tags.map((tag) => (
                    <label key={tag.id} className="tag-option">
                      <input
                        type="checkbox"
                        checked={tagIds.includes(tag.id)}
                        onChange={() => toggleTag(tag.id)}
                      />
                      <TagChip tag={tag} />
                    </label>
                  ))}
                </div>
              )}
            </fieldset>

            <label className="checkbox">
              <input
                type="checkbox"
                checked={production}
                onChange={(e) => setProduction(e.target.checked)}
              />
              <span>{t("hostDialog.production")}</span>
            </label>
            <p className="hint">{t("hostDialog.productionHint")}</p>
          </>
        )}

        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
