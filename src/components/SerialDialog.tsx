import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  isTauri,
  type SerialConfig,
  type SerialFlowControl,
  type SerialParity,
  type SerialPortInfo,
} from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

const BAUD_RATES = [
  4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
];

export function SerialDialog() {
  const { t } = useTranslation();
  const openSession = useAppStore((s) => s.openSession);
  const pluginScan = useAppStore((s) => s.pluginScan);
  const serialInstalled = !!pluginScan?.plugins.some(
    (plugin) => plugin.kind === "serial",
  );
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [path, setPath] = useState("");
  const [baudRate, setBaudRate] = useState(115200);
  const [dataBits, setDataBits] = useState(8);
  const [parity, setParity] = useState<SerialParity>("none");
  const [stopBits, setStopBits] = useState(1);
  const [flowControl, setFlowControl] = useState<SerialFlowControl>("none");
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState("");

  const refreshPorts = useCallback(async () => {
    if (!isTauri) return;
    setLoading(true);
    try {
      const list = await api.serialListPorts();
      setPorts(list);
      setLoadError("");
      setPath((current) => current || list[0]?.path || "");
    } catch (err) {
      setLoadError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refreshPorts();
  }, [refreshPorts]);

  const close = () => useAppStore.getState().setSerialDialogOpen(false);
  const validPath = path.trim().length > 0;
  const canConnect = isTauri && serialInstalled && validPath;

  const submit = () => {
    if (!canConnect) return;
    const config: SerialConfig = {
      path: path.trim(),
      baudRate,
      dataBits,
      parity,
      stopBits,
      flowControl,
    };
    const name = path.trim().split(/[\\/]/).pop() || path.trim();
    openSession("serial", `${name} · ${baudRate}`, {
      type: "serial",
      config,
    });
    close();
  };

  return (
    <Modal title={t("serial.title")} onClose={close}>
      <form
        className="form-grid"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <p className="hint">{t("serial.hint")}</p>
        {!serialInstalled && (
          <p className="hint warn">{t("serial.needsPlugin")}</p>
        )}
        {!isTauri && <p className="hint warn">{t("serial.desktopOnly")}</p>}
        <label className="field">
          <span>{t("serial.port")}</span>
          <div className="field-inline">
            <input
              list="serial-port-options"
              value={path}
              autoFocus
              placeholder="/dev/tty.usbserial-1120"
              onChange={(event) => setPath(event.target.value)}
            />
            <datalist id="serial-port-options">
              {ports.map((port) => (
                <option key={port.path} value={port.path}>
                  {port.name}
                </option>
              ))}
            </datalist>
            <button
              type="button"
              onClick={() => void refreshPorts()}
              disabled={loading || !isTauri}
            >
              {t("serial.refreshPorts")}
            </button>
          </div>
          {isTauri && !loading && ports.length === 0 && !loadError && (
            <span className="hint">{t("serial.noPorts")}</span>
          )}
          {loadError && <span className="hint warn">{loadError}</span>}
        </label>
        <label className="field">
          <span>{t("serial.baudRate")}</span>
          <div className="field-inline">
            <input
              type="number"
              min={50}
              max={4000000}
              step={1}
              list="serial-baud-rates"
              value={baudRate}
              onChange={(event) => setBaudRate(Number(event.target.value))}
            />
            <datalist id="serial-baud-rates">
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
              value={dataBits}
              onChange={(event) => setDataBits(Number(event.target.value))}
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
              value={stopBits}
              onChange={(event) => setStopBits(Number(event.target.value))}
            >
              <option value={1}>1</option>
              <option value={2}>2</option>
            </select>
          </label>
          <label className="field">
            <span>{t("serial.parity")}</span>
            <select
              value={parity}
              onChange={(event) =>
                setParity(event.target.value as SerialParity)
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
              value={flowControl}
              onChange={(event) =>
                setFlowControl(event.target.value as SerialFlowControl)
              }
            >
              <option value="none">{t("serial.flowNone")}</option>
              <option value="rtscts">{t("serial.flowRtsCts")}</option>
              <option value="xonxoff">{t("serial.flowXonXoff")}</option>
            </select>
          </label>
        </div>
        <div className="actions">
          <button type="button" onClick={close}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="primary" disabled={!canConnect}>
            {t("serial.connect")}
          </button>
        </div>
      </form>
    </Modal>
  );
}
