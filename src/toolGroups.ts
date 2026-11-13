import type { NetToolId } from "./store";
import {
  FilesIcon,
  ForwardIcon,
  GlobeIcon,
  KeyIcon,
  ListIcon,
  PulseIcon,
  SearchIcon,
  ShieldIcon,
  SignalIcon,
  SlidersIcon,
  TagIcon,
  ThemeIcon,
} from "./components/icons";

/**
 * Single source of truth for the toolbox. The sidebar builds its groups from
 * this and the content page dispatches on the same ids, so a tool can never
 * appear in the list without a matching page (or vice versa).
 */
export type ToolGroupId = "network" | "frontend" | "encoding" | "text";

export type NetworkToolId = "ping" | "geoip" | "ipcalc";
export type UtilityToolId = Exclude<NetToolId, NetworkToolId>;

export interface ToolEntry {
  id: NetToolId;
  icon: typeof GlobeIcon;
}

export interface ToolGroup {
  id: ToolGroupId;
  tools: ToolEntry[];
}

export const TOOL_GROUPS: ToolGroup[] = [
  {
    id: "network",
    tools: [
      { id: "ping", icon: SignalIcon },
      { id: "geoip", icon: GlobeIcon },
      { id: "ipcalc", icon: SlidersIcon },
    ],
  },
  {
    id: "frontend",
    tools: [
      { id: "json", icon: ListIcon },
      { id: "color", icon: ThemeIcon },
      { id: "regex", icon: SearchIcon },
    ],
  },
  {
    id: "encoding",
    tools: [
      { id: "base64", icon: KeyIcon },
      { id: "url", icon: ForwardIcon },
      { id: "timestamp", icon: PulseIcon },
    ],
  },
  {
    id: "text",
    tools: [
      { id: "counter", icon: FilesIcon },
      { id: "hash", icon: ShieldIcon },
      { id: "uuid", icon: TagIcon },
    ],
  },
];

export const ALL_TOOL_IDS: NetToolId[] = TOOL_GROUPS.flatMap((group) =>
  group.tools.map((tool) => tool.id),
);

/** Narrows the selected id to the three tools that need the desktop backend. */
export function isNetworkTool(id: NetToolId): id is NetworkToolId {
  return id === "ping" || id === "geoip" || id === "ipcalc";
}
