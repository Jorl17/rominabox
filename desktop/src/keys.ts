import keyboard from "../keyboard.json";

// The keys we capture in the builder, by KeyboardEvent.code, and how we word
// a key, as declared in keyboard.json. We check every name there with the
// RetroArch parser in the exporter tests.
const capture: Record<string, string> = keyboard.capture;
const labels: Record<string, string> = keyboard.labels;

/** How we word a stored key name, wherever we show one. */
export function keyName(key: string): string {
  return labels[key] || key.toUpperCase();
}

/** The name we store for a pressed key, or null for a key we do not capture. */
export function capturedKey(event: KeyboardEvent): string | null {
  return capture[event.code] || null;
}
