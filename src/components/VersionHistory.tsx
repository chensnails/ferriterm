import { Fragment, useState } from "react";
import { useTranslation } from "react-i18next";
import i18n from "../i18n";
import { clockLabel, dayLabel, groupByDay, parseStamp } from "../timeline";
import { BUILD_HEAD, BUILD_HISTORY } from "../version";
import { Chip, DaySeparator, EmptyState, PanelHeader } from "./ui";

const VISIBLE_COUNT = 12;

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (!Number.isFinite(diff)) return "";
  const formatter = new Intl.RelativeTimeFormat(i18n.language, {
    numeric: "auto",
  });
  const minutes = Math.round(diff / 60000);
  if (Math.abs(minutes) < 60) return formatter.format(-minutes, "minute");
  const hours = Math.round(diff / 3600000);
  if (Math.abs(hours) < 24) return formatter.format(-hours, "hour");
  return formatter.format(-Math.round(diff / 86400000), "day");
}

export function VersionHistory() {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const total = BUILD_HISTORY.length;

  if (total === 0) {
    return (
      <section className="version-history">
        <PanelHeader title={t("about.historyTitle")} />
        <EmptyState title={t("about.historyEmpty")} />
      </section>
    );
  }

  const visible = expanded ? BUILD_HISTORY : BUILD_HISTORY.slice(0, VISIBLE_COUNT);
  // Commit dates span several days once the list grows; day headings keep the
  // timeline readable and pin to the top while the list scrolls.
  const groups = groupByDay(visible, (entry) => parseStamp(entry.date));

  return (
    <section className="version-history">
      <PanelHeader
        title={t("about.historyTitle")}
        actions={
          <span className="hint">
            {t("about.historyCount", { count: total })}
          </span>
        }
      />
      <p className="hint version-history-hint">{t("about.historyHint")}</p>
      <ol className="version-timeline" aria-label={t("about.historyTitle")}>
        {groups.map((group) => (
          <Fragment key={group.key}>
            <DaySeparator
              as="li"
              sticky
              label={
                group.key === "unknown"
                  ? t("time.unknownDay")
                  : dayLabel(group.date)
              }
              count={group.items.length}
            />
            {group.items.map((entry) => {
              const current = entry.hash === BUILD_HEAD;
              const date = parseStamp(entry.date);
              const relative = relativeTime(entry.date);
              return (
                <li
                  key={entry.hash}
                  className={current ? "version-entry current" : "version-entry"}
                >
                  <span className="version-node" aria-hidden />
                  <div className="version-entry-body">
                    <div className="version-entry-head">
                      <code className="version-label">{entry.label}</code>
                      {current && <Chip tone="info">{t("about.current")}</Chip>}
                    </div>
                    <p className="version-subject">{entry.subject}</p>
                    <p className="hint version-time">
                      <time dateTime={entry.date}>
                        {date ? clockLabel(date) : entry.date.slice(11, 16)}
                      </time>
                      {relative && (
                        <>
                          <span aria-hidden> · </span>
                          <span>{relative}</span>
                        </>
                      )}
                    </p>
                  </div>
                </li>
              );
            })}
          </Fragment>
        ))}
      </ol>
      {total > VISIBLE_COUNT && !expanded && (
        <button
          type="button"
          className="version-show-all"
          onClick={() => setExpanded(true)}
        >
          {t("about.showAll", { count: total })}
        </button>
      )}
    </section>
  );
}
