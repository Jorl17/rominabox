import { useEffect, useState, type RefObject } from "react";
import { ChevronDown } from "lucide-react";

/** An arrow on the top rule of the navigation while part of the step is
 * below the visible area. Clicking it scrolls the area down. */
export function MoreBelow({
  area,
  step,
}: {
  area: RefObject<HTMLElement | null>;
  step: number;
}) {
  const [below, setBelow] = useState(false);
  useEffect(() => {
    const element = area.current;
    if (!element) return;
    let frame = 0;
    // We measure at most once a frame, after the layout of any change.
    const measure = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() =>
        setBelow(
          element.scrollTop + element.clientHeight < element.scrollHeight - 1,
        ),
      );
    };
    // The part below changes with scrolling, the window's size and the step's
    // content, such as a disclosure that opens or a picture that loads.
    const changes = new MutationObserver(measure);
    changes.observe(element, {
      subtree: true,
      childList: true,
      attributes: true,
      characterData: true,
    });
    element.addEventListener("scroll", measure, { passive: true });
    element.addEventListener("load", measure, true);
    window.addEventListener("resize", measure);
    measure();
    return () => {
      cancelAnimationFrame(frame);
      changes.disconnect();
      element.removeEventListener("scroll", measure);
      element.removeEventListener("load", measure, true);
      window.removeEventListener("resize", measure);
    };
  }, [area, step]);
  return (
    <button
      type="button"
      className="more-below"
      aria-hidden="true"
      tabIndex={-1}
      hidden={!below}
      onClick={() => {
        const element = area.current;
        element?.scrollBy({
          top: element.clientHeight * 0.8,
          behavior: "smooth",
        });
      }}
    >
      <ChevronDown size={16} />
    </button>
  );
}
