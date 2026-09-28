import declared from "../defaults.json";
import inc from "../../vendor/retroarch/menu/drivers/rmlui/menu_controls.inc?raw";
import type { Pressed } from "./bindingCapture";
import { keyName } from "./keys";
import { padPositions, positionName } from "./padPositions";

/** The bindings of each action of the menu, by the id of the action. */
export type MenuControls = typeof declared.menuControls;
export type MenuAction = keyof MenuControls;
/** The builder defaults, in the order of the actions. */
export const defaultMenuControls: MenuControls = declared.menuControls;
export const menuActions = Object.keys(defaultMenuControls) as MenuAction[];

// How a binding is written, as menu_controls.inc declares it for the game.
function declaration(name: string): string[] {
  const found = inc.match(new RegExp(`^${name}\\((.*)\\)$`, "m"));
  if (!found) throw new Error(`menu_controls.inc declares no ${name}`);
  return found[1].split(",").map((field) => field.trim().replace(/^"|"$/g, ""));
}
const [, keyPrefix] = /^RIB_MENU_BINDING\(Key, "([^"]*)"\)$/m.exec(inc) ?? [];
const [, padPrefix] = /^RIB_MENU_BINDING\(Pad, "([^"]*)"\)$/m.exec(inc) ?? [];
const [chord] = declaration("RIB_MENU_PAD_CHORD");
const [homeId, , homeWord] = declaration("RIB_MENU_PAD_HOME");

export function actionName(action: string): string {
  return action.charAt(0).toUpperCase() + action.slice(1);
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

/** A broken rule of the game's menu (`menu_controls::Refusal`). */
export type Refusal =
  | { kind: "noBinding" | "noKey"; action: string }
  | { kind: "twice"; action: string; binding: string }
  | { kind: "shared"; action: string; other: string; binding: string };

/** A refusal as the author reads it, having changed `changed`. */
export function refusalWords(refusal: Refusal, changed: string): string {
  switch (refusal.kind) {
    case "noBinding":
      return `${actionName(refusal.action)} needs a binding.`;
    case "noKey":
      return `${actionName(refusal.action)} needs a key.`;
    case "twice":
      return `${actionName(refusal.action)} already has ${bindingWords(refusal.binding)}.`;
    case "shared": {
      const holder =
        refusal.action === changed ? refusal.other : refusal.action;
      return `${bindingWords(refusal.binding)} is already ${actionName(holder)}'s.`;
    }
  }
}
