import { useTranslation } from "react-i18next";
import { useAppStore, type NetToolId } from "../store";
import { GlobeIcon, SignalIcon, SlidersIcon } from "./icons";
import { ListRow, PanelHeader } from "./ui";

const TOOLS: { id: NetToolId; icon: typeof GlobeIcon }[] = [
  { id: "ping", icon: SignalIcon },
  { id: "geoip", icon: GlobeIcon },
  { id: "ipcalc", icon: SlidersIcon },
];

/** Sidebar content for the network toolbox module. */
export function ToolList() {
  const { t } = useTranslation();
  const netTool = useAppStore((s) => s.netTool);
  const setNetTool = useAppStore((s) => s.setNetTool);

  return (
    <>
      <PanelHeader
        title={
          <>
            {t("tools.title")}
            <span className="head-count"> ({TOOLS.length})</span>
          </>
        }
      />
      <div className="tool-list">
        {TOOLS.map((tool) => {
          const Icon = tool.icon;
          return (
            <ListRow
              key={tool.id}
              icon={<Icon size={16} />}
              title={t(`tools.${tool.id}.title`)}
              meta={t(`tools.${tool.id}.hint`)}
              selected={netTool === tool.id}
              onClick={() => setNetTool(tool.id)}
            />
          );
        })}
      </div>
      <p className="hint tool-list-foot">{t("tools.localHint")}</p>
    </>
  );
}
