import type React from "react";
import { FileImage, X } from "lucide-react";
import { Checkbox } from "./Help";

/** The picture behind the game's menu: add, change or remove it, and, once
 * there is one, whether we draw it in the colour of the palette. */
export function BackgroundPicker({
  chosen,
  onChoose,
  onDrop,
  onRemove,
  tint,
  onTint,
}: {
  chosen: boolean;
  onChoose: () => void;
  onDrop: (e: React.DragEvent) => void;
  onRemove: () => void;
  tint: boolean;
  onTint: (tint: boolean) => void;
}) {
  return (
    <>
      <div className="customize-row">
        <div
          className="background-picker"
          data-drop="background"
          onDragOver={(e) => e.preventDefault()}
          onDrop={onDrop}
        >
          <button className="secondary" onClick={onChoose}>
            <FileImage size={17} />
            {chosen ? "Change background" : "Add background"}
          </button>
          {chosen && (
            <button
              className="icon-button"
              aria-label="Remove background"
              onClick={onRemove}
            >
              <X size={18} />
            </button>
          )}
        </div>
      </div>
      {chosen && (
        <Checkbox
          label="Tint background"
          checked={tint}
          onChange={onTint}
          help="Draw the picture in the palette's colour, so the menu's text reads on any picture. Off, the picture shows as it is."
        />
      )}
    </>
  );
}
