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

export function PlusIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M8 3.5v9M3.5 8h9" />
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
      <circle cx="8" cy="8" r="2.2" />
      <path d="M8 2.2v1.4M8 12.4v1.4M2.2 8h1.4M12.4 8h1.4M3.9 3.9l1 1M11.1 11.1l1 1M12.1 3.9l-1 1M4.9 11.1l-1 1" />
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

export function ForwardIcon({ size = 14 }: { size?: number }) {
  return (
    <svg {...base(size)}>
      <path d="M2.5 5.5h8M8.5 3.5L10.5 5.5 8.5 7.5" />
      <path d="M13.5 10.5h-8M7 8.5l-2 2 2 2" />
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


