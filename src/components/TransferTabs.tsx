interface TransferTabsProps {
  tabs: string[];
  activeId: string;
  /** Display label per tab id, already disambiguated by the caller. */
  labelOf: (id: string) => string;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onOpenPicker: (rect: DOMRect) => void;
  copyTo: string;
  copyTargets: string[];
  onCopyToChange: (id: string) => void;
  labels: {
    local: string;
    copyTo: string;
    open: string;
    close: string;
    empty: string;
  };
}

/**
 * Tab strip for the remote side of the transfer view. One pane, N targets:
 * the tabs only switch which target the pane shows, they never stack panes.
 * The strip scrolls sideways, while the "+" and the copy target stay put.
 */
export function TransferTabs({
  tabs,
  activeId,
  labelOf,
  onSelect,
  onClose,
  onOpenPicker,
  copyTo,
  copyTargets,
  onCopyToChange,
  labels,
}: TransferTabsProps) {
  return (
    <div className="transfer-tabs">
      <div className="transfer-tabs-scroll" role="tablist">
        {tabs.map((id) => (
          <div
            key={id}
            role="tab"
            aria-selected={id === activeId}
            className={id === activeId ? "transfer-tab active" : "transfer-tab"}
          >
            <button
              type="button"
              className="transfer-tab-label"
              title={labelOf(id)}
              onClick={() => onSelect(id)}
            >
              {labelOf(id)}
            </button>
            {tabs.length > 1 && (
              <button
                type="button"
                className="transfer-tab-close"
                title={labels.close}
                aria-label={labels.close}
                onClick={() => onClose(id)}
              >
                ×
              </button>
            )}
          </div>
        ))}
        {tabs.length === 0 && (
          <span className="transfer-tab-empty">{labels.empty}</span>
        )}
      </div>
      <div className="transfer-tabs-actions">
        <button
          type="button"
          className="transfer-tab-add"
          title={labels.open}
          aria-label={labels.open}
          onClick={(event) =>
            onOpenPicker(event.currentTarget.getBoundingClientRect())
          }
        >
          ＋
        </button>
        {copyTargets.length > 0 && (
          <label className="transfer-copyto">
            <span>{labels.copyTo}</span>
            <select
              value={copyTo}
              title={copyTo === "local" ? labels.local : labelOf(copyTo)}
              onChange={(event) => onCopyToChange(event.target.value)}
            >
              <option value="local">{labels.local}</option>
              {copyTargets.map((id) => (
                <option key={id} value={id}>
                  {labelOf(id)}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
    </div>
  );
}
