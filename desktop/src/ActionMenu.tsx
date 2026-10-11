import { EllipsisVertical } from "lucide-react";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import "./ActionMenu.css";

/** One choice in an ActionMenu, or a line between groups of choices. */
export type MenuItem =
  | {
      label: string;
      /** A short second line under the label. */
      detail?: string;
      icon?: ReactNode;
      danger?: boolean;
      disabled?: boolean;
      onSelect: () => void;
    }
  | "separator";

/** The part of the window where `element` shows: the box of the nearest
 * folder around it that scrolls, within the window. */
function visibleBounds(element: HTMLElement): { top: number; bottom: number } {
  let bounds = { top: 0, bottom: window.innerHeight };
  for (let at = element.parentElement; at; at = at.parentElement) {
    const scrolls = /(auto|scroll)/.test(getComputedStyle(at).overflowY);
    if (scrolls) {
      const box = at.getBoundingClientRect();
      bounds = {
        top: Math.max(bounds.top, box.top),
        bottom: Math.min(bounds.bottom, box.bottom),
      };
      break;
    }
  }
  return bounds;
}

/** A vertical ellipsis that opens a list of actions under it, or above it
 * when there is more room there and not enough under it. A click outside,
 * Escape or a choice closes it. */
export function ActionMenu({
  label,
  items,
}: {
  label: string;
  items: MenuItem[];
}) {
  const [open, setOpen] = useState(false);
  const [above, setAbove] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  const list = useRef<HTMLDivElement>(null);

  // The list opens under the ellipsis, and before the browser draws it we
  // move it above when it does not fit under and there is more room above.
  useLayoutEffect(() => {
    if (!open || !root.current || !list.current) {
      setAbove(false);
      return;
    }
    const button = root.current.getBoundingClientRect();
    const under = list.current.getBoundingClientRect();
    const bounds = visibleBounds(root.current);
    const fits = under.bottom <= bounds.bottom;
    setAbove(!fits && button.top - bounds.top > bounds.bottom - button.bottom);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const outside = (event: MouseEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  return (
    <span
      className="action-menu"
      ref={root}
      onClick={(event) => event.stopPropagation()}
    >
      <button
        type="button"
        className="icon-button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <EllipsisVertical size={18} />
      </button>
      {open && (
        <div
          ref={list}
          className={above ? "action-menu-list above" : "action-menu-list"}
          role="menu"
          aria-label={label}
        >
          {items.map((item, index) =>
            item === "separator" ? (
              <div
                key={index}
                className="action-menu-separator"
                role="separator"
              />
            ) : (
              <button
                key={item.label}
                type="button"
                role="menuitem"
                className={
                  item.danger ? "action-menu-item danger" : "action-menu-item"
                }
                disabled={item.disabled}
                onClick={() => {
                  setOpen(false);
                  item.onSelect();
                }}
              >
                {item.icon && (
                  <span className="action-menu-icon">{item.icon}</span>
                )}
                <span className="action-menu-words">
                  <span className="action-menu-label">{item.label}</span>
                  {item.detail && (
                    <span className="action-menu-detail">{item.detail}</span>
                  )}
                </span>
              </button>
            ),
          )}
        </div>
      )}
    </span>
  );
}
