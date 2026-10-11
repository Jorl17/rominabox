import "./MiddleEllipsis.css";

/** `text` on one line. When it does not fit, we cut it in the middle and
 * keep its last part, the name of a file in a path, whole. */
export function MiddleEllipsis({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  const cut = Math.max(text.lastIndexOf("/"), text.lastIndexOf("\\")) + 1;
  return (
    <span
      className={className ? `middle-ellipsis ${className}` : "middle-ellipsis"}
    >
      <span className="middle-ellipsis-start">{text.slice(0, cut)}</span>
      <span className="middle-ellipsis-end">{text.slice(cut)}</span>
    </span>
  );
}
