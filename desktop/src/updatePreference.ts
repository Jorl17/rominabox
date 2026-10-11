/** Whether we look for a newer builder when the builder starts. It is on
 * unless the person turned it off in About, and we keep the choice in the
 * storage of the webview. */
const KEY = "rominabox.checkForUpdates";

export function checksForUpdates(): boolean {
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

export function setChecksForUpdates(on: boolean): void {
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {
    // Without storage, the choice lasts until the builder closes.
  }
}
