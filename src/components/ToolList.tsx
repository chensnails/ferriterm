import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { ALL_TOOL_IDS, TOOL_GROUPS } from "../toolGroups";
import { ListRow, PanelHeader, SectionHeader } from "./ui";

/** Sidebar content for the toolbox module: one section per tool group. */
export function ToolList() {
  const { t } = useTranslation();
  const netTool = useAppStore((s) => s.netTool);
  const setNetTool = useAppStore((s) => s.setNetTool);

  return (
    <>
      <PanelHeader
        dragRegion
        title={
          <>
            {t("tools.title")}
            <span className="head-count"> ({ALL_TOOL_IDS.length})</span>
          </>
        }
      />
      <div className="tool-list">
        {TOOL_GROUPS.map((group) => (
          <section key={group.id} className="tool-group">
            <SectionHeader
              title={t(`tools.groups.${group.id}`)}
              count={group.tools.length}
            />
            {group.tools.map((tool) => {
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
          </section>
        ))}
      </div>
      <p className="hint tool-list-foot">{t("tools.localHint")}</p>
    </>
  );
}
