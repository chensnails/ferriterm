import { useCallback, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

/**
 * Model picker drawn by the app instead of the WebView.
 *
 * A native `<datalist>` popup ignores the theme, the type scale and the radius
 * system, so the field owns a listbox that anchors to the input and follows
 * scroll. Typing is also a free-text escape hatch: a model the vendor's
 * catalogue omits (a fresh alias, a private deployment) can still be entered by
 * hand.
 */
export function ModelCombo({
  value,
  options,
  onChange,
  placeholder,
  disabled,
  ariaLabel,
  className,
}: {
  value: string;
  options: string[];
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  ariaLabel?: string;
  className?: string;
}) {
  const listboxId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [box, setBox] = useState<{
    left: number;
    top: number;
    width: number;
    flip: boolean;
  } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const popRef = useRef<HTMLDivElement>(null);

  // Typing filters the catalogue; a value that already names one entry counts
  // as "chosen", so reopening the list offers the whole catalogue again.
  const query = value.trim().toLowerCase();
  const chosen = options.some((option) => option.toLowerCase() === query);
  const list = (
    query.length > 0 && !chosen
      ? options.filter((option) => option.toLowerCase().includes(query))
      : options
  ).slice(0, 300);

  const place = useCallback(() => {
    const input = inputRef.current;
    if (!input) return;
    const rect = input.getBoundingClientRect();
    const room = 260;
    const below = window.innerHeight - rect.bottom;
    const flip = below < room && rect.top > below;
    setBox({
      left: rect.left,
      top: flip ? rect.top - 6 : rect.bottom + 6,
      width: rect.width,
      flip,
    });
  }, []);

  useEffect(() => {
    if (!open) return;
    place();
    const onScroll = () => place();
    const onDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (inputRef.current?.contains(target)) return;
      if (popRef.current?.contains(target)) return;
      setOpen(false);
    };
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("mousedown", onDown);
    };
  }, [open, place]);

  useEffect(() => {
    setActive(0);
  }, [value, open]);

  const choose = (model: string) => {
    onChange(model);
    setOpen(false);
    inputRef.current?.focus();
  };

  const visible = open && list.length > 0 && box !== null;

  return (
    <div className={className ? `ai-combo ${className}` : "ai-combo"}>
      <input
        ref={inputRef}
        className="ai-field"
        role="combobox"
        aria-expanded={visible}
        aria-controls={visible ? listboxId : undefined}
        aria-label={ariaLabel}
        aria-autocomplete="list"
        autoComplete="off"
        placeholder={placeholder}
        disabled={disabled}
        value={value}
        onChange={(event) => {
          onChange(event.target.value);
          setOpen(true);
        }}
        onFocus={() => {
          if (options.length > 0) setOpen(true);
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            if (options.length === 0) return;
            event.preventDefault();
            if (!open) {
              setOpen(true);
              return;
            }
            const delta = event.key === "ArrowDown" ? 1 : -1;
            setActive((current) => (current + delta + list.length) % list.length);
          } else if (event.key === "Enter" && visible) {
            event.preventDefault();
            choose(list[Math.min(active, list.length - 1)]);
          } else if (event.key === "Escape" && open) {
            // Keep the surrounding dialog open: the modal listens on window.
            event.preventDefault();
            event.stopPropagation();
            setOpen(false);
          } else if (event.key === "Tab") {
            setOpen(false);
          }
        }}
      />
      {visible &&
        createPortal(
          <div
            ref={popRef}
            id={listboxId}
            role="listbox"
            aria-label={ariaLabel ?? value}
            className={`ai-combo-popup${box.flip ? " flip" : ""}`}
            style={{ left: box.left, top: box.top, width: box.width }}
            onMouseDown={(event) => event.preventDefault()}
          >
            {list.map((model, index) => (
              <button
                key={model}
                type="button"
                role="option"
                aria-selected={model === value}
                className={[
                  "ai-combo-option",
                  index === active ? "active" : "",
                  model === value ? "selected" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                onMouseEnter={() => setActive(index)}
                onClick={() => choose(model)}
              >
                {model}
              </button>
            ))}
          </div>,
          // Scroll containers clip their own overflow, so the list has to live
          // outside them to stay whole.
          document.body,
        )}
    </div>
  );
}
