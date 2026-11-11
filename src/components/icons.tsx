import { useId } from "react";

const base = (size: number) => ({
  width: size,
  height: size,
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
});
export function LogIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="2.5" y="2" width="11" height="12" rx="1.5" />
      <path d="M5 5.5h6M5 8h6M5 10.5h4" />
    </svg>
  );
}

export function LogoMark({ size = 32 }: { size?: number }) {
  const id = useId();
  const bg = `ft-logo-bg-${id}`;
  const fg = `ft-logo-fg-${id}`;
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
      <defs>
        <linearGradient id={bg} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#081120" />
          <stop offset="1" stopColor="#0d2c4a" />
        </linearGradient>
        <linearGradient id={fg} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor="#6ee7f9" />
          <stop offset="1" stopColor="#4f8cff" />
        </linearGradient>
      </defs>
      <rect width="32" height="32" rx="6.8" fill={`url(#${bg})`} />
      <path
        d="M10.2 9.9 16.2 16l-6 6.1"
        fill="none"
        stroke={`url(#${fg})`}
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M17.1 22.1h4.7"
        fill="none"
        stroke={`url(#${fg})`}
        strokeWidth="3"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function PlusIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M8 3.5v9M3.5 8h9" />
    </svg>
  );
}

export function SidebarIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="2" y="2.5" width="12" height="11" rx="1.5" />
      <path d="M6 2.5v11" />
    </svg>
  );
}

export function FolderPlusIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.8l1.4 1.5h4.8A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
      <path d="M8 7.5v4M6 9.5h4" />
    </svg>
  );
}

export function TagIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2.5 3.5v3.6c0 .4.2.8.4 1l5.4 5.4a1.4 1.4 0 0 0 2 0l2.8-2.8a1.4 1.4 0 0 0 0-2L7.7 3.3a1.4 1.4 0 0 0-1-.4H3.5a1 1 0 0 0-1 1z" />
      <circle cx="5.6" cy="6.2" r="1" />
    </svg>
  );
}

export function SearchIcon({ size = 13 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <circle cx="7" cy="7" r="4.2" />
      <path d="M10.2 10.2L13.5 13.5" />
    </svg>
  );
}

export function Chevron({ open, size = 12 }: { open: boolean; size?: number }) {
  return (
    <svg className={open ? "chev open" : "chev"} {...base(size)}>
      <path d="M6 4l4 4-4 4" />
    </svg>
  );
}

export function FolderIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
    </svg>
  );
}

export function GearIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M14.79 6.31 14.79 9.69 13.01 9.73 12.76 10.32 14 11.61 11.61 14 10.32 12.76 9.73 13.01 9.69 14.79 6.31 14.79 6.27 13.01 5.68 12.76 4.39 14 2 11.61 3.24 10.32 2.99 9.73 1.21 9.69 1.21 6.31 2.99 6.27 3.24 5.68 2 4.39 4.39 2 5.68 3.24 6.27 2.99 6.31 1.21 9.69 1.21 9.73 2.99 10.32 3.24 11.61 2 14 4.39 12.76 5.68 13.01 6.27Z" />
      <circle cx="8" cy="8" r="2.4" />
    </svg>
  );
}

export function PulseIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2 8h2.6l1.6-4.2 2.6 8.4 1.7-4.2H14" />
    </svg>
  );
}

export function ListIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M5.5 4.5h8M5.5 8h8M5.5 11.5h8" />
      <path d="M2.5 4.5h.01M2.5 8h.01M2.5 11.5h.01" strokeWidth={2} />
    </svg>
  );
}

export function CpuIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="4.5" y="4.5" width="7" height="7" rx="1.2" />
      <path d="M6.5 2v2.5M9.5 2v2.5M6.5 11.5V14M9.5 11.5V14M2 6.5h2.5M2 9.5h2.5M11.5 6.5H14M11.5 9.5H14" />
    </svg>
  );
}

export function MemoryIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="2" y="4.5" width="12" height="7" rx="1.2" />
      <path d="M5 6.5v3M8 6.5v3M11 6.5v3" />
    </svg>
  );
}

export function DiskIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="2" y="3.5" width="12" height="9" rx="1.5" />
      <path d="M2 9.5h12" />
      <path d="M10.5 11.5h1" strokeWidth={2} />
    </svg>
  );
}

export function NetworkIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M5 3.5v6M5 9.5l-2 2M5 9.5l2 2" />
      <path d="M11 12.5v-6M11 6.5l-2-2M11 6.5l2-2" />
    </svg>
  );
}

export function ForwardIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2.5 5.5h8M8.5 3.5L10.5 5.5 8.5 7.5" />
      <path d="M13.5 10.5h-8M7 8.5l-2 2 2 2" />
    </svg>
  );
}

export function PlayIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <circle cx="8" cy="8" r="6" />
      <path d="M6.8 5.4l3.8 2.6-3.8 2.6z" />
    </svg>
  );
}

export function BroadcastIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <circle cx="8" cy="8" r="1.4" fill="currentColor" stroke="none" />
      <path d="M5.2 5.2a4 4 0 0 0 0 5.6M10.8 5.2a4 4 0 0 1 0 5.6" />
      <path d="M3 3a7 7 0 0 0 0 10M13 3a7 7 0 0 1 0 10" />
    </svg>
  );
}

export function SplitIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="2" y="2.5" width="12" height="11" rx="1.5" />
      <path d="M8 2.5v11" />
    </svg>
  );
}

export function FilesIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.8l1.4 1.5h4.8A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
      <path d="M5.5 9h5M8.5 7.5L10 9l-1.5 1.5" />
    </svg>
  );
}

/** Simplified home glyph: roof plus body, no door or chimney detail. */
export function HomeIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2.5 7.6 8 3.2l5.5 4.4" />
      <path d="M4.3 6.9v5.9h7.4V6.9" />
    </svg>
  );
}

export function TerminalIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M4.5 6l2 2-2 2M8.5 10h3" />
    </svg>
  );
}

export function SlidersIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
      <circle cx="6" cy="4.5" r="1.5" fill="currentColor" stroke="none" />
      <circle cx="10" cy="8" r="1.5" fill="currentColor" stroke="none" />
      <circle cx="5" cy="11.5" r="1.5" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function KeyboardIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="1.5" y="4" width="13" height="8" rx="1.5" />
      <path d="M4 6.5h.01M6.5 6.5h.01M9 6.5h.01M11.5 6.5h.01M4 9h.01M6.5 9h.01M9 9h.01M11.5 9h.01M5.5 10.8h5" />
    </svg>
  );
}

export function PuzzleIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M3 3h4v1.2a1.3 1.3 0 1 0 2 0V3h4v4h-1.2a1.3 1.3 0 1 0 0 2H13v4H9v-1.2a1.3 1.3 0 1 0-2 0V13H3V9h1.2a1.3 1.3 0 1 0 0-2H3z" />
    </svg>
  );
}

export function CloudIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M4.5 12h7a2.7 2.7 0 0 0 .3-5.4A4 4 0 0 0 4.2 7.2 2.5 2.5 0 0 0 4.5 12z" />
      <path d="M8 8v3M6.8 9.2L8 8l1.2 1.2" />
    </svg>
  );
}

export function InfoIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <circle cx="8" cy="8" r="6" />
      <path d="M8 7v4M8 5h.01" />
    </svg>
  );
}

export function EyeIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M1.5 8s2.3-3.5 6.5-3.5S14.5 8 14.5 8 12.2 11.5 8 11.5 1.5 8 1.5 8z" />
      <circle cx="8" cy="8" r="1.8" />
    </svg>
  );
}

/** Globe: the network toolbox module. */
export function GlobeIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <circle cx="8" cy="8" r="6" />
      <path d="M2 8h12M8 2c1.7 1.8 2.6 3.8 2.6 6S9.7 12.2 8 14c-1.7-1.8-2.6-3.8-2.6-6S6.3 3.8 8 2z" />
    </svg>
  );
}

/** Signal bars: ping / traceroute probes. */
export function SignalIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2 11v2M5.5 8.5v4.5M9 6v7M12.5 3v10" />
    </svg>
  );
}

export function KeyIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <circle cx="5" cy="8" r="2.6" />
      <path d="M7.4 8h6.1M11.2 8v2.4M13 8v1.8" />
    </svg>
  );
}

export function CloseIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M4 4l8 8M12 4l-8 8" />
    </svg>
  );
}

/** Solid dot: the tab bar's session recording toggle. */
export function RecordIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <circle cx="8" cy="8" r="4.5" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function StopIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect
        x="4.5"
        y="4.5"
        width="7"
        height="7"
        rx="1.2"
        fill="currentColor"
        stroke="none"
      />
    </svg>
  );
}

export function PauseIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M6 4v8M10 4v8" strokeWidth={2} />
    </svg>
  );
}

/** Circular arrow: reconnect a dropped session or refresh a listing. */
export function ReconnectIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M13 8a5 5 0 1 1-1.6-3.7" />
      <path d="M13.2 2.6v3.1h-3.1" />
    </svg>
  );
}

export function RefreshIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M3 8a5 5 0 0 1 8.6-3.5M13 8a5 5 0 0 1-8.6 3.5" />
      <path d="M11.8 2.4v2.3H9.5M4.2 13.6v-2.3h2.3" />
    </svg>
  );
}

/** Half-lit disc: switch between the light and dark themes. */
export function ThemeIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 2.5a5.5 5.5 0 0 1 0 11z" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function DownloadIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M8 2.5v7M5.2 7l2.8 2.8L10.8 7M3 12.7h10" />
    </svg>
  );
}

export function UploadIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M8 10V3M5.2 5.8L8 3l2.8 2.8M3 12.7h10" />
    </svg>
  );
}

export function CheckIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M3.2 8.4l3 3 6.5-6.8" />
    </svg>
  );
}

export function CopyIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <rect x="5.5" y="5.5" width="8" height="8" rx="1.4" />
      <path d="M10.5 5.5v-2a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2" />
    </svg>
  );
}

export function TrashIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.3 4.5l.6 8.1a1 1 0 0 0 1 .9h4.2a1 1 0 0 0 1-.9l.6-8.1" />
      <path d="M6.7 7v4M9.3 7v4" />
    </svg>
  );
}



export function SparkleIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M8 1.8l1.3 3.4a2 2 0 0 0 1.2 1.2l3.4 1.3-3.4 1.3a2 2 0 0 0-1.2 1.2L8 13.6l-1.3-3.4a2 2 0 0 0-1.2-1.2L2.1 7.7l3.4-1.3a2 2 0 0 0 1.2-1.2z" />
    </svg>
  );
}

export function ShieldIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M8 1.8l5 1.9v3.6c0 3-2.1 5-5 6.1-2.9-1.1-5-3.1-5-6.1V3.7z" />
      <path d="M5.8 7.6l1.6 1.6 3-3.2" />
    </svg>
  );
}
