import keyboard from "../keyboard.json";
import keyWords from "../../vendor/retroarch/menu/drivers/rmlui/key_words.inc?raw";

// The keys we capture in the builder, by KeyboardEvent.code, as declared in
// keyboard.json, and how we word a key, as declared in key_words.inc for the
// game's menu too. We check every name in both with the RetroArch parser in
// the exporter tests.
const capture: Record<string, string> = keyboard.capture;
const words: Record<string, string> = Object.fromEntries(
  [...keyWords.matchAll(/^RIB_KEY_WORD\("([^"]+)", "([^"]*)"\)$/gm)].map(
    ([, name, word]) => [name, word],
  ),
);

/** How we word a stored key name, wherever we show one. */
export function keyName(key: string): string {
  return words[key] || key.toUpperCase();
}

/** The name we store for a pressed key, or null for a key we do not capture. */
export function capturedKey(event: KeyboardEvent): string | null {
  return capture[event.code] || null;
}
