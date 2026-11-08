import { useEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  onSelect?: () => void;
  danger?: boolean;
  checked?: boolean;
  children?: MenuItem[];
}

function moveIn(container: HTMLElement, delta: number) {
  const buttons = Array.from(
    container.querySelectorAll(":scope > button, :scope > .menu-sub > button"),
  );
  if (buttons.length === 0) return;
  const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
  const next = (current + delta + buttons.length) % buttons.length;
  (buttons[next] as HTMLButtonElement).focus();
}

function MenuEntry({
  item,
  onClose,
  open,
  flip,
  onOpen,
  onCloseSub,
}: {
  item: MenuItem;
  onClose: () => void;
  open: boolean;
  flip: boolean;
  onOpen: () => void;
  onCloseSub: () => void;
}) {
  const hasChildren = !!item.children?.length;
  const itemClass = ["menu-item", item.danger ? "danger" : ""].filter(Boolean).join(" ");
  const submenuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    submenuRef.current?.querySelector("button")?.focus();
  }, [open]);

  const activate = () => {
    if (hasChildren) {
      onOpen();
      return;
    }
    item.onSelect?.();
    onClose();
  };

  return (
    <div
      className="menu-sub"
      onMouseEnter={hasChildren ? onOpen : undefined}
      onMouseLeave={() => {
        if (!hasChildren || !open) return;
        if (submenuRef.current?.contains(document.activeElement)) {
          triggerRef.current?.focus();
        }
        onCloseSub();
      }}
    >
      <button
        ref={triggerRef}
        role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
        aria-checked={item.checked}
        aria-haspopup={hasChildren ? "menu" : undefined}
        aria-expanded={hasChildren ? open : undefined}
        className={itemClass}
        onClick={activate}
        onKeyDown={(e) => {
          if (hasChildren && (e.key === "ArrowRight" || e.key === "Enter")) {
            e.preventDefault();
            onOpen();
          }
        }}
      >
        {item.checked !== undefined && (
          <span className="menu-check" aria-hidden="true">
            {item.checked ? "✓" : ""}
          </span>
        )}
        <span className="menu-label">{item.label}</span>
        {hasChildren && (
          <span className="menu-arrow" aria-hidden="true">
            ›
          </span>
        )}
      </button>

      {hasChildren && open && (
        <div
          ref={submenuRef}
          className={`context-menu submenu${flip ? " flip" : ""}`}
          role="menu"
          aria-label={item.label}
          onKeyDown={(e) => {
            if (!submenuRef.current) return;
            if (e.key === "ArrowDown") {
              e.preventDefault();
              e.stopPropagation();
              moveIn(submenuRef.current, 1);
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              e.stopPropagation();
              moveIn(submenuRef.current, -1);
            } else if (e.key === "ArrowLeft") {
              e.preventDefault();
              e.stopPropagation();
              triggerRef.current?.focus();
              onCloseSub();
            }
          }}
        >
          {item.children?.map((child) => (
            <MenuEntry
              key={child.label}
              item={child}
              onClose={onClose}
              open={false}
              flip={false}
              onOpen={() => {}}
              onCloseSub={() => {}}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [openIndex, setOpenIndex] = useState<number | null>(null);

  useEffect(() => {
    ref.current?.querySelector("button")?.focus();
  }, []);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  const left = Math.min(x, window.innerWidth - 200);
  const top = Math.min(y, window.innerHeight - items.length * 34 - 12);
  const flip = left + 200 > window.innerWidth - 180;

  return (
    <div
      ref={ref}
      className="context-menu"
      role="menu"
      style={{ left, top }}
      onMouseLeave={() => setOpenIndex(null)}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          setOpenIndex(null);
          moveIn(ref.current as HTMLElement, e.key === "ArrowDown" ? 1 : -1);
        }
      }}
    >
      {items.map((item, index) => (
        <MenuEntry
          key={item.label}
          item={item}
          onClose={onClose}
          open={openIndex === index}
          flip={flip}
          onOpen={() => setOpenIndex(index)}
          onCloseSub={() => setOpenIndex(null)}
        />
      ))}
    </div>
  );
}
