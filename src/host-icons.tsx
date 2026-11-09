import type { HostProtocol } from "./api";
import { FilesIcon, TerminalIcon } from "./components/icons";

export interface HostIconDef {
  id: string;
  name: string;
  group: "system" | "network" | "device";
  color: string;
  letters: string;
}

/**
 * Original monogram badges for the systems and vendors operators actually
 * label in a host tree. Brand names stay in Latin script, so no translation
 * keys are needed; every badge is drawn from data rather than copied artwork.
 */
export const HOST_ICONS: HostIconDef[] = [
  { id: "ubuntu", name: "Ubuntu", group: "system", color: "#e95420", letters: "U" },
  { id: "debian", name: "Debian", group: "system", color: "#a80030", letters: "D" },
  { id: "armbian", name: "Armbian", group: "system", color: "#c8102e", letters: "A" },
  { id: "raspbian", name: "Raspberry Pi OS", group: "system", color: "#c51a4a", letters: "R" },
  { id: "centos", name: "CentOS", group: "system", color: "#932279", letters: "C" },
  { id: "redhat", name: "Red Hat", group: "system", color: "#ee0000", letters: "RH" },
  { id: "fedora", name: "Fedora", group: "system", color: "#294172", letters: "F" },
  { id: "rocky", name: "Rocky Linux", group: "system", color: "#10b981", letters: "RL" },
  { id: "alma", name: "AlmaLinux", group: "system", color: "#0f4266", letters: "AL" },
  { id: "opensuse", name: "openSUSE", group: "system", color: "#73ba25", letters: "SUSE" },
  { id: "arch", name: "Arch Linux", group: "system", color: "#1793d1", letters: "AR" },
  { id: "alpine", name: "Alpine Linux", group: "system", color: "#0d597f", letters: "AP" },
  { id: "linux", name: "Linux", group: "system", color: "#3f4b5b", letters: "LX" },
  { id: "macos", name: "macOS", group: "system", color: "#4a4a4a", letters: "MAC" },
  { id: "windows", name: "Windows", group: "system", color: "#0078d4", letters: "WIN" },
  { id: "freebsd", name: "FreeBSD", group: "system", color: "#ab2b28", letters: "BSD" },
  { id: "huawei", name: "Huawei", group: "network", color: "#c7000b", letters: "HW" },
  { id: "h3c", name: "H3C", group: "network", color: "#e60012", letters: "H3C" },
  { id: "cisco", name: "Cisco", group: "network", color: "#1ba0d7", letters: "CS" },
  { id: "ruijie", name: "Ruijie", group: "network", color: "#009944", letters: "RG" },
  { id: "zte", name: "ZTE", group: "network", color: "#0068b7", letters: "ZTE" },
  { id: "tplink", name: "TP-Link", group: "network", color: "#4acbd6", letters: "TP" },
  { id: "mikrotik", name: "MikroTik", group: "network", color: "#293239", letters: "MT" },
  { id: "fortinet", name: "Fortinet", group: "network", color: "#da291c", letters: "FT" },
  { id: "juniper", name: "Juniper", group: "network", color: "#84b135", letters: "JN" },
  { id: "router", name: "Router", group: "device", color: "", letters: "" },
  { id: "switch", name: "Switch", group: "device", color: "", letters: "" },
  { id: "firewall", name: "Firewall", group: "device", color: "", letters: "" },
  { id: "access-point", name: "Access point", group: "device", color: "", letters: "" },
  { id: "server", name: "Server", group: "device", color: "", letters: "" },
  { id: "nas", name: "NAS", group: "device", color: "", letters: "" },
  { id: "printer", name: "Printer", group: "device", color: "", letters: "" },
];

const strokeProps = (size: number) => ({
  width: size,
  height: size,
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.4,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
});

function DeviceGlyph({ id, size }: { id: string; size: number }) {
  const props = strokeProps(size);
  switch (id) {
    case "router":
      return (
        <svg {...props}>
          <rect x="1.8" y="6" width="12.4" height="6" rx="1.4" />
          <path d="M4 9h1.5M7 9h1.5M10.5 9h1.5M4.5 6V4M11.5 6V4M3 3.2l1.5.8M13 3.2l-1.5.8" />
        </svg>
      );
    case "switch":
      return (
        <svg {...props}>
          <rect x="1.8" y="4" width="12.4" height="8" rx="1.4" />
          <path d="M4 7.2h2M7.5 7.2h2M11 7.2h1M4 9.6h2M7.5 9.6h2M11 9.6h1" />
        </svg>
      );
    case "firewall":
      return (
        <svg {...props}>
          <path d="M8 1.8l5.2 2.1v4.2c0 3-2.2 5.2-5.2 6.1-3-.9-5.2-3.1-5.2-6.1V3.9L8 1.8z" />
          <path d="M5.2 7.2h5.6M8 4.6v5.2" />
        </svg>
      );
    case "access-point":
      return (
        <svg {...props}>
          <rect x="5.4" y="9.2" width="5.2" height="4.4" rx="1.1" />
          <path d="M4 6.6a5.6 5.6 0 0 1 8 0M2.2 4.4a8.4 8.4 0 0 1 11.6 0M8 9.2V7.8" />
        </svg>
      );
    case "server":
      return (
        <svg {...props}>
          <rect x="2.4" y="2" width="11.2" height="4.2" rx="1.1" />
          <rect x="2.4" y="9.8" width="11.2" height="4.2" rx="1.1" />
          <path d="M4.4 4.1h1.2M4.4 11.9h1.2M10 4.1h1.4M10 11.9h1.4" />
        </svg>
      );
    case "nas":
      return (
        <svg {...props}>
          <rect x="2.4" y="2.2" width="11.2" height="11.6" rx="1.4" />
          <path d="M2.4 6.1h11.2M2.4 9.9h11.2M4.6 4.2h1.2M4.6 8h1.2M4.6 11.8h1.2" />
        </svg>
      );
    case "printer":
      return (
        <svg {...props}>
          <path d="M4.6 6V2.4h6.8V6" />
          <rect x="2" y="6" width="12" height="5.2" rx="1.2" />
          <path d="M4.8 9.4h6.4v4.2H4.8z" />
        </svg>
      );
    default:
      return <TerminalIcon size={size} />;
  }
}

function SerialGlyph({ size }: { size: number }) {
  return (
    <svg {...strokeProps(size)}>
      <path d="M1.8 8h3.4M10.8 8h3.4" />
      <rect x="5.2" y="5.6" width="5.6" height="4.8" rx="1" />
      <path d="M6.6 5.6V3.9M9.4 5.6V3.9M6.6 10.4v1.7M9.4 10.4v1.7" />
    </svg>
  );
}

export function HostIcon({
  icon,
  protocol,
  size = 16,
}: {
  icon?: string;
  protocol?: HostProtocol;
  size?: number;
}) {
  const def = icon ? HOST_ICONS.find((entry) => entry.id === icon) : undefined;
  if (def) {
    if (def.group === "device") return <DeviceGlyph id={def.id} size={size} />;
    const font = size * (def.letters.length > 2 ? 0.31 : 0.44);
    return (
      <span
        className="host-icon-badge"
        style={{
          width: size,
          height: size,
          background: def.color,
          fontSize: font,
          borderRadius: Math.max(3, size * 0.24),
        }}
        aria-hidden="true"
      >
        {def.letters}
      </span>
    );
  }
  if (protocol === "sftp") return <FilesIcon size={size} />;
  if (protocol === "serial") return <SerialGlyph size={size} />;
  return <TerminalIcon size={size} />;
}
