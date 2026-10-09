import {
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { Info } from "lucide-react";

/** A tooltip for the element in `anchor`, above it where there is room and
 * below it otherwise, placed in the viewport so that it stays readable inside
 * scrolling panels. Escape closes it. */
function useTip<T extends HTMLElement>() {
  const id = useId();
  const anchor = useRef<T>(null);
  const tip = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  useLayoutEffect(() => {
    if (!open) return;
    const position = () => {
      if (!anchor.current || !tip.current) return;
      const box = anchor.current.getBoundingClientRect();
      const popup = tip.current.getBoundingClientRect();
      const left = Math.max(
        12,
        Math.min(
          box.left + box.width / 2 - popup.width / 2,
          window.innerWidth - popup.width - 12,
        ),
      );
      const above = box.top - popup.height - 10;
      tip.current.style.left = `${left}px`;
      tip.current.style.top = `${Math.max(12, Math.min(above >= 12 ? above : box.bottom + 10, window.innerHeight - popup.height - 12))}px`;
    };
    position();
    window.addEventListener("resize", position);
    window.addEventListener("scroll", position, true);
    const dismiss = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", dismiss);
    return () => {
      window.removeEventListener("resize", position);
      window.removeEventListener("scroll", position, true);
      window.removeEventListener("keydown", dismiss);
    };
  }, [open]);
  const popup = (children: ReactNode) =>
    open &&
    createPortal(
      <div ref={tip} className="help-tooltip" role="tooltip" id={id}>
        {children}
      </div>,
      document.body,
    );
  return { id, anchor, open, setOpen, popup };
}

/** Viewport-level help remains readable inside scrolling panels. */
export function Help({
  children,
  label = "More information",
}: {
  children: ReactNode;
  label?: string;
}) {
  const { id, anchor, open, setOpen, popup } = useTip<HTMLButtonElement>();
  return (
    <span
      className="help"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        ref={anchor}
        type="button"
        className="help-button"
        aria-label={label}
        aria-describedby={open ? id : undefined}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onClick={() => setOpen(true)}
      >
        <Info size={16} />
      </button>
      {popup(children)}
    </span>
  );
}

/** A button that shows only its icon, with its words in a tooltip, as Help
 * shows its text. The tooltip also shows while the button is off, to say
 * why. */
export function IconButton({
  label,
  tip,
  className = "",
  disabled = false,
  onClick,
  children,
}: {
  label: string;
  tip: ReactNode;
  className?: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  const { id, anchor, open, setOpen, popup } = useTip<HTMLSpanElement>();
  return (
    <span
      ref={anchor}
      className="icon-button-anchor"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        type="button"
        className={`icon-button ${className}`}
        aria-label={label}
        aria-describedby={open ? id : undefined}
        disabled={disabled}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onClick={(event) => {
          event.stopPropagation();
          onClick();
        }}
      >
        {children}
      </button>
      {popup(tip)}
    </span>
  );
}

/** The label and its help are one line of words beside the box, so the help
 * follows the last word however the words wrap. The help is outside the
 * label so that pressing it does not tick the box. */
export function Checkbox({
  label,
  checked,
  disabled = false,
  onChange,
  help,
  className = "",
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
  help?: ReactNode;
  className?: string;
}) {
  const id = useId();
  return (
    <div className={`option-toggle ${className}`}>
      <input
        id={id}
        className="checkbox-box"
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="option-words">
        <label className="checkbox" htmlFor={id}>
          {label}
        </label>
        {help && <Help label={`About ${label.toLowerCase()}`}>{help}</Help>}
      </span>
    </div>
  );
}
