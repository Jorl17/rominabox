import React, { useRef } from "react";
import * as bridge from "./bridge";
import { Help } from "./Help";

/**
 * The BIOS files we bundle with a game. For a console whose BIOS is optional,
 * the button is labelled Optional, and we describe the BIOS in its help.
 */
export function FirmwarePicker({
  files,
  onChange,
  blocked,
  assessment,
  optionalHelp,
  onError,
}: {
  files: string[];
  onChange: React.Dispatch<React.SetStateAction<string[]>>;
  blocked: boolean;
  assessment: bridge.FirmwareAssessment | null;
  optionalHelp: string | undefined;
  onError: (reason: unknown) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <div className="firmware-picker">
      <button
        type="button"
        className="secondary"
        onClick={async () => {
          if (!bridge.native) {
            input.current?.click();
            return;
          }
          try {
            const paths = await bridge.pickFirmware();
            onChange((current) => [...new Set([...current, ...paths])]);
          } catch (e) {
            onError(e);
          }
        }}
      >
        {blocked && files.length === 0
          ? "Choose BIOS files"
          : `Add BIOS files${optionalHelp ? " (optional)" : ""}`}
      </button>
      <input
        ref={input}
        type="file"
        hidden
        multiple
        data-firmware
        onChange={(e) => {
          const names = [...(e.target.files ?? [])].map((file) => file.name);
          onChange((current) => [...new Set([...current, ...names])]);
          e.target.value = "";
        }}
      />
      <Help>{optionalHelp ?? "Choose the BIOS files for this console."}</Help>
      {files.map((path) => {
        const name = path.split(/[\\/]/).pop() || path;
        const reported = assessment?.files.find((file) => file.name === name);
        return (
          <div className="firmware-file" key={path}>
            <span>{name}</span>
            <button
              type="button"
              className="text-button"
              aria-label={"Remove " + name}
              onClick={() =>
                onChange((current) => current.filter((file) => file !== path))
              }
            >
              Remove
            </button>
            {reported?.reason && (
              <p className="firmware-reason">{reported.reason}</p>
            )}
          </div>
        );
      })}
    </div>
  );
}
