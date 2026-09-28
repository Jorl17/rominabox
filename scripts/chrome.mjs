/* The Google Chrome already on this machine, for the scripts that drive a page
 * headless. We download nothing. The places where Chrome is installed, by
 * platform. On Windows these are the machine's and the person's program
 * folders, with their Windows names. */
import fs from "node:fs";
import path from "node:path";

const WINDOWS_CHROME = path.join("Google", "Chrome", "Application", "chrome.exe");

const LOCATIONS = {
  darwin: () => [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
  ],
  linux: () => [
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ],
  win32: () =>
    [process.env.ProgramFiles, process.env["ProgramFiles(x86)"], process.env.LOCALAPPDATA]
      .filter(Boolean)
      .map((folder) => path.join(folder, WINDOWS_CHROME)),
};

/* How we start Chrome to drive a page from a script: headless, muted (on the
 * builder's menu step we play previews of the sound packs), with the
 * switches of the script. */
export function chromeLaunch(switches = []) {
  return {
    executablePath: findChrome(),
    headless: true,
    args: ["--mute-audio", ...switches],
  };
}

export function findChrome() {
  const locations = LOCATIONS[process.platform];
  if (!locations) {
    throw new Error(`No Chrome location is declared for ${process.platform}.`);
  }
  const found = locations().find((candidate) => fs.existsSync(candidate));
  if (!found) {
    throw new Error(
      "Google Chrome is not installed. This uses the browser already on the " +
        "machine and does not download one.",
    );
  }
  return found;
}
