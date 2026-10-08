import declared from "../defaults.json";
import type { Hotkeys } from "./hotkeys";

/** The defaults of the builder, from desktop/defaults.json. In TypeScript an empty
 * JSON list has the type `never[]`, a list with no possible members, so we
 * type every hotkey as a list of bindings. */
export const builderDefaults: Omit<typeof declared, "hotkeys"> & {
  hotkeys: Hotkeys;
} = declared;
