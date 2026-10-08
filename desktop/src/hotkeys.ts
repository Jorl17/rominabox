import declared from "../defaults.json";
import inc from "../../vendor/retroarch/menu/drivers/rmlui/hotkeys.inc?raw";
import wordsInc from "../../vendor/retroarch/menu/drivers/rmlui/words.inc?raw";
import type { Pressed } from "./bindingCapture";
import { keyName } from "./keys";
import { padPositions, positionName } from "./padPositions";

/** Each hotkey's bindings, by the hotkey's id. */
export type Hotkeys = Record<keyof typeof declared.hotkeys, string[]>;
export type Hotkey = keyof Hotkeys;
/** The builder's defaults. */
export const defaultHotkeys: Hotkeys = declared.hotkeys;

// The hotkeys, and how a binding is written, as hotkeys.inc declares them
// for the game.
function declarations(name: string): string[][] {
  return [...inc.matchAll(new RegExp(`^${name}\\((.*)\\)$`, "gm"))].map(
    (found) =>
      found[1].split(",").map((field) => field.trim().replace(/^"|"$/g, "")),
  );
}
function declaration(name: string): string[] {
  const [found] = declarations(name);
  if (!found) throw new Error(`hotkeys.inc declares no ${name}`);
  return found;
}
/** The id of every hotkey, in the order of the declarations for the game. */
export const hotkeyIds = declarations("RIB_HOTKEY").map(
  ([, id]) => id as Hotkey,
);
const prefix = (kind: string) =>
  declarations("RIB_HOTKEY_BINDING").find(([name]) => name === kind)?.[1] ?? "";
const keyPrefix = prefix("Key");
const padPrefix = prefix("Pad");
const [chord] = declaration("RIB_HOTKEY_PAD_CHORD");
const [homeId, , homeWord] = declaration("RIB_HOTKEY_PAD_HOME");

/** The hotkeys that the player can also use with the fullscreen chord of the
 * platform, which they cannot remove (RIB_HOTKEY_CHORD in hotkeys.inc). */
export const chordHotkeys: Hotkey[] = declarations("RIB_HOTKEY_CHORD").map(
  ([name]) =>
    declarations("RIB_HOTKEY").find(
      ([declared]) => declared === name,
    )?.[1] as Hotkey,
);
function word(id: string): string {
  const found = wordsInc.match(
    new RegExp(`RIB_WORD\\(\\w+, "${id}", "([^"]*)"\\)`),
  );
  if (!found) throw new Error(`words.inc declares no ${id}`);
  return found[1];
}
/** The fullscreen chord for the platform of this computer, from the words of
 * the game, written as we write a chord here, for example "Alt + Enter". In a
 * game for the other platform, the player sees the chord of that platform. */
export const chordWords = word(
  /Mac/.test(globalThis.navigator?.platform ?? "")
    ? "fullscreen-chord-mac"
    : "fullscreen-chord",
)
  .split(chord)
  .map((part) => part[0] + part.slice(1).toLowerCase())
  .join(` ${chord} `);

/** A hotkey as the author reads it: its id in words, "Quick save". */
export function hotkeyName(id: string): string {
  const words = id.replace(/-/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** A binding in the words the game shows it in. */
export function bindingWords(binding: string): string {
  if (binding.startsWith(keyPrefix))
    return keyName(binding.slice(keyPrefix.length));
  return binding
    .slice(padPrefix.length)
    .split(chord)
    .map((id) => (id === homeId ? homeWord : positionName(id)))
    .join(` ${chord} `);
}

/** What a press binds: a key or a pad button. Null for a stick's direction,
 * which we do not read in the menu. */
export function bindingOf(pressed: Pressed): string | null {
  if (pressed.kind === "key") return keyPrefix + pressed.key;
  const position = padPositions.find((each) => each.id === pressed.position);
  return position && !position.opposite ? padPrefix + position.id : null;
}

/** A broken rule of the game's menu (`hotkeys::Refusal`). */
export type Refusal =
  | { kind: "noBinding" | "noKey"; hotkey: string }
  | { kind: "twice"; hotkey: string; binding: string }
  | { kind: "shared"; hotkey: string; other: string; binding: string }
  | {
      kind: "gameInput";
      hotkey: string;
      binding: string;
      control: string;
      label: string;
    }
  | { kind: "controls"; message: string };

/**
 * A refusal as the author reads it, having changed the hotkey `changed`, or
 * a control of the game when there is none.
 */
export function refusalWords(refusal: Refusal, changed?: string): string {
  switch (refusal.kind) {
    case "noBinding":
      return `${hotkeyName(refusal.hotkey)} needs a binding.`;
    case "noKey":
      return `${hotkeyName(refusal.hotkey)} needs a key.`;
    case "twice":
      return `${hotkeyName(refusal.hotkey)} already has ${bindingWords(refusal.binding)}.`;
    case "shared": {
      const holder =
        refusal.hotkey === changed ? refusal.other : refusal.hotkey;
      return `${bindingWords(refusal.binding)} is already ${hotkeyName(holder)}'s.`;
    }
    case "gameInput": {
      const words = bindingWords(refusal.binding);
      if (refusal.hotkey !== changed)
        return `${words} is already ${hotkeyName(refusal.hotkey)}'s.`;
      const what = refusal.binding.startsWith(keyPrefix) ? "key" : "button";
      return `${words} is the game's ${what} for ${refusal.label}.`;
    }
    case "controls":
      return refusal.message;
  }
}
