/**
 * Drive the built builder in headless Chrome and exit.
 *
 * We use the browser already installed and download none, and we close the
 * browser and the server before returning.
 */
import { createRequire } from "node:module";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(
  new URL("../desktop/package.json", import.meta.url),
);
const { chromium } = require("playwright-core");

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".wav": "audio/wav",
  ".woff2": "font/woff2",
};

function argument(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? null : process.argv[index + 1];
}

function findChrome() {
  const candidates = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  ];
  const found = candidates.find((candidate) => fs.existsSync(candidate));
  if (!found) {
    throw new Error(
      "Google Chrome is not installed. This uses the browser already on the machine and does not download one.",
    );
  }
  return found;
}

function serve(root) {
  const server = createServer((request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    let file = path.resolve(root, `.${decodeURIComponent(url.pathname)}`);
    if (file !== root && !file.startsWith(root + path.sep)) {
      response.writeHead(403);
      response.end();
      return;
    }
    if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      const fallback = path.join(root, "index.html");
      if (path.extname(file) === "" && fs.existsSync(fallback)) file = fallback;
      else {
        response.writeHead(404);
        response.end();
        return;
      }
    }
    response.writeHead(200, {
      "Content-Type": MIME[path.extname(file)] || "application/octet-stream",
    });
    fs.createReadStream(file).pipe(response);
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

function writeRom() {
  const directory = path.join(ROOT, "work/test-output/builder-shots");
  fs.mkdirSync(directory, { recursive: true });
  const file = path.join(directory, "SONIC THE HEDGEHOG.md");
  const data = Buffer.alloc(512);
  data.write("SEGA", 0x100, "ascii");
  data.write("SONIC THE HEDGEHOG", 336, "ascii");
  fs.writeFileSync(file, data);
  return file;
}

function profiles() {
  const controls = JSON.parse(
    fs.readFileSync(path.join(ROOT, "desktop/controls.json"), "utf8"),
  );
  const systems = JSON.parse(
    fs.readFileSync(path.join(ROOT, "desktop/systems.json"), "utf8"),
  ).systems;
  const firmware = new Set(
    systems.filter((system) => system.firmware).map((system) => system.id),
  );
  return controls.profiles.map((profile) => {
    let candidates = profile.systems.filter((id) => !firmware.has(id));
    if (profile.id === "retropad") {
      candidates = systems
        .filter(
          (system) =>
            system.controllerProfile === "retropad" && !firmware.has(system.id),
        )
        .map((system) => system.id);
    }
    return {
      id: profile.id,
      name: profile.name,
      image: profile.image,
      system: candidates[0] ?? null,
    };
  });
}

async function shot(page, file) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  await page.screenshot({ path: file, animations: "disabled" });
  console.log(`shot ${path.relative(ROOT, file)}`);
}

async function clickNext(page) {
  const next = page.getByRole("button", { name: "Next", exact: true });
  await next.waitFor();
  await page.waitForFunction(() => {
    const button = [...document.querySelectorAll("button")].find((item) =>
      item.textContent.includes("Next"),
    );
    return button && !button.disabled;
  });
  await next.click();
}

async function dropRom(page, rom) {
  const bytes = [...fs.readFileSync(rom)];
  const name = path.basename(rom);
  await page.locator("[data-drop='game']").evaluate(
    (zone, payload) => {
      const file = new File([new Uint8Array(payload.bytes)], payload.name);
      const data = new DataTransfer();
      data.items.add(file);
      zone.dispatchEvent(
        new DragEvent("dragover", { bubbles: true, dataTransfer: data }),
      );
      zone.dispatchEvent(
        new DragEvent("drop", { bubbles: true, dataTransfer: data }),
      );
    },
    { bytes, name },
  );
}

async function waitForMenuImage(page, palette) {
  await page.waitForFunction((id) => {
    const image = document.querySelector(".menu-frame img");
    return (
      image &&
      (image.getAttribute("src") || "").includes(id) &&
      image.complete &&
      image.naturalWidth > 0
    );
  }, palette);
}

async function openShaders(page) {
  const shaders = page
    .locator("details.advanced")
    .filter({ has: page.locator(".shader-choices") });
  await shaders.waitFor();
  const open = await shaders.evaluate((element) => element.open);
  if (!open) await shaders.locator("summary").click();
  return shaders;
}

async function shaderText(page) {
  return page
    .locator(".shader-choices")
    .evaluate((element) => element.textContent || "");
}

async function openControls(page) {
  const details = page.locator("details.author-controls");
  await details.waitFor();
  if (!(await details.evaluate((element) => element.open))) {
    await details.locator("summary").first().click();
  }
  await page.locator(".controls-editor").waitFor();
}

async function shootEditor(page, file) {
  await page.evaluate(() => {
    for (const selector of [".app-shell", "main", ".screen"]) {
      const element = document.querySelector(selector);
      element.dataset.shotHeight = element.style.height;
      element.dataset.shotOverflow = element.style.overflow;
      element.style.height = "auto";
      element.style.overflow = "visible";
    }
  });
  const editor = page.locator(".controls-editor");
  await editor.evaluate((element) => element.scrollIntoView({ block: "start" }));
  const box = await editor.boundingBox();
  if (!box) throw new Error("the controls editor has no box to photograph");
  await page.setViewportSize({
    width: 1440,
    height: Math.min(4800, Math.ceil(box.y + box.height + 24)),
  });
  fs.mkdirSync(path.dirname(file), { recursive: true });
  await editor.screenshot({ path: file, animations: "disabled" });
  await page.evaluate(() => {
    for (const selector of [".app-shell", "main", ".screen"]) {
      const element = document.querySelector(selector);
      element.style.height = element.dataset.shotHeight;
      element.style.overflow = element.dataset.shotOverflow;
    }
  });
  await page.setViewportSize({ width: 1440, height: 900 });
  console.log(`shot ${path.relative(ROOT, file)}`);
}

async function quoteHelp(page) {
  const count = await page.locator(".help-button").count();
  for (let index = 0; index < count; index += 1) {
    const button = page.locator(".help-button").nth(index);
    if (!(await button.isVisible())) continue;
    const label = await button.getAttribute("aria-label");
    await button.focus();
    const tip = page.locator(".help-tooltip");
    try {
      await tip.waitFor({ timeout: 2000 });
    } catch {
      continue;
    }
    const text = (await tip.innerText()).replace(/\s+/g, " ").trim();
    console.log(`HELP ${label}: ${text}`);
    await page.keyboard.press("Escape");
  }
}

async function main() {
  const checking = process.argv.includes("--check");
  const dist = argument("--dist") ? path.resolve(argument("--dist")) : null;
  const out = argument("--out") ? path.resolve(argument("--out")) : null;
  if (!dist || (!checking && !out)) {
    throw new Error("pass --dist and either --check or --out");
  }

  const rom = writeRom();
  const server = await serve(dist);
  const address = server.address();
  const browser = await chromium.launch({
    executablePath: findChrome(),
    headless: true,
  });
  let code = 0;
  try {
    const page = await browser.newPage({
      viewport: { width: 1440, height: 900 },
    });
    page.setDefaultTimeout(20000);
    await page.goto(`http://127.0.0.1:${address.port}/`, {
      waitUntil: "networkidle",
    });
    await page.getByRole("heading", { name: "Choose a game" }).waitFor();

    if (!checking) await shot(page, path.join(out, "01-game.png"));
    await page.locator("[data-drop='game']").evaluate((zone) => {
      zone.dispatchEvent(new DragEvent("dragover", { bubbles: true }));
    });
    if (!checking) await shot(page, path.join(out, "02-game-dragging.png"));
    await dropRom(page, rom);
    await page.getByRole("heading", { name: "SONIC THE HEDGEHOG.md" }).waitFor();
    if (!checking) await shot(page, path.join(out, "03-game-dropped.png"));

    await clickNext(page);
    await page.getByRole("heading", { name: "Game details" }).waitFor();
    await page.waitForFunction(() => {
      const title = document.querySelector(".fields input");
      const system = document.querySelector(".fields select");
      return title && title.value.includes("SONIC") && system && system.value;
    });
    const identified = await page.locator(".fields input").inputValue();
    const system = await page.locator(".fields select").inputValue();
    console.log(`IDENTIFIED title=${JSON.stringify(identified)} system=${system}`);
    if (!checking) {
      await shot(page, path.join(out, "04-details.png"));
      await page
        .locator("details.advanced summary", { hasText: "More details" })
        .click();
      await shot(page, path.join(out, "05-details-more.png"));
      await quoteHelp(page);
    }

    await clickNext(page);
    await page.getByRole("heading", { name: "Choose a menu" }).waitFor();
    const designs = await page.getByLabel("Menu design").evaluate((select) =>
      [...select.options].map((option) => option.textContent.trim()),
    );
    console.log(`DESIGNS ${designs.join(" | ")}`);

    if (!checking) {
      for (const palette of ["blue", "green", "amber"]) {
        await page
          .getByRole("button", { name: palette[0].toUpperCase() + palette.slice(1) })
          .click();
        await waitForMenuImage(page, palette);
        const src = await page.locator(".menu-frame img").getAttribute("src");
        console.log(`PREVIEW ${palette} ${src}`);
        const file = path.join(out, `06-menu-${palette}.png`);
        await shot(page, file);
        const image = path.join(out, `06-menu-${palette}-frame.png`);
        await page.locator(".menu-frame img").screenshot({ path: image });
        const digest = createHash("sha256")
          .update(fs.readFileSync(image))
          .digest("hex")
          .slice(0, 12);
        console.log(`PREVIEW HASH ${palette} ${digest}`);
      }
      await page.getByRole("button", { name: "Blue" }).click();
      await waitForMenuImage(page, "blue");

      await page
        .getByRole("button", { name: "About startup logo", exact: true })
        .hover();
      await page.locator(".help-tooltip").waitFor();
      await shot(page, path.join(out, "07-menu-help.png"));
      await page.keyboard.press("Escape");

      await page.locator(".menu-settings details.advanced summary").click();
      const character = (
        (await page.locator(".sound-character").textContent()) || ""
      )
        .replace(/\s+/g, " ")
        .trim();
      console.log(`VISIBLE sound: ${character}`);
      await shot(page, path.join(out, "08-menu-customize.png"));
      await quoteHelp(page);
    } else {
      await waitForMenuImage(page, "blue");
      const src = await page.locator(".menu-frame img").getAttribute("src");
      console.log(`PREVIEW blue ${src}`);
    }

    if (!checking) {
      const customize = page.locator(".menu-settings details.advanced");
      if (await customize.evaluate((element) => element.open)) {
        await customize.locator("summary").click();
      }
    }

    await openShaders(page);
    const shaders = (await shaderText(page)).replace(/\s+/g, " ").trim();
    const pictures = await page.locator(".shader-choices img").count();
    console.log(`SHADERS pictures=${pictures} text=${JSON.stringify(shaders)}`);
    const missing = ["Scanlines", "Phosphor"].filter(
      (name) => !shaders.includes(name),
    );
    if (missing.length) {
      console.error("shader packaging is not on the menu step");
      console.error(`missing ${missing.join(", ")}`);
      code = 1;
      return;
    }
    // An <img> that does not load shows as an empty box, not an error, so
    // counting them would not catch broken pictures. We require each preview
    // to have loaded and to have pixels.
    const blank = await page
      .locator(".shader-choices img")
      .evaluateAll((images) =>
        images
          .filter((image) => !image.complete || image.naturalWidth === 0)
          .map((image) => image.getAttribute("src") || "(no src)"),
      );
    if (pictures < 2 || blank.length) {
      console.error("a picture filter has no preview a person can see");
      console.error(`pictures=${pictures} blank=${JSON.stringify(blank)}`);
      code = 1;
      return;
    }
    if (!checking) {
      await shot(page, path.join(out, "09-shaders.png"));
      await page.getByRole("checkbox", { name: "Scanlines" }).check();
      await page.getByRole("checkbox", { name: "Phosphor" }).check();
      await page.getByLabel("Starts on").selectOption("phosphor");
      const shadersRow = page.locator(".shader-choices");
      await shadersRow.scrollIntoViewIfNeeded();
      await shot(page, path.join(out, "10-shaders-chosen.png"));
      await shadersRow.screenshot({
        path: path.join(out, "10-shaders-row.png"),
        animations: "disabled",
      });
      console.log("shot docs/reports/builder/10-shaders-row.png");
      await quoteHelp(page);
      await page
        .locator("details.advanced")
        .filter({ has: page.locator(".shader-choices") })
        .locator("summary")
        .click();
    }

    if (!checking) {
      const menu = page.getByRole("checkbox", { name: "Include game menu" });
      await menu.uncheck();
      await page.getByText("No in-game menu.").waitFor();
      await shot(page, path.join(out, "11-menu-off.png"));
      await menu.check();
      await waitForMenuImage(page, "blue");
    }

    await openControls(page);
    await page.waitForFunction(() => {
      const image = document.querySelector("svg.controller-scene image");
      return (image?.getAttribute("href") || "").endsWith(".svg");
    });
    const drawing = await page
      .locator("svg.controller-scene image")
      .getAttribute("href");
    console.log(`DRAWING default ${drawing}`);

    if (checking) {
      await clickNext(page);
      await page.getByRole("heading", { name: "Export your game" }).waitFor();
      const note = await page.locator(".note").innerText();
      const create = page.getByRole("button", { name: "Create app", exact: true });
      if (!(await create.isDisabled()) || !note.includes("desktop app")) {
        console.error("export is not the disabled browser step");
        code = 1;
        return;
      }
      console.log("builder check ok");
      return;
    }

    await shot(page, path.join(out, "12-controls.png"));
    await shootEditor(page, path.join(out, "controllers/megadrive.png"));

    await clickNext(page);
    await page.getByRole("heading", { name: "Export your game" }).waitFor();
    await shot(page, path.join(out, "13-export.png"));
    await quoteHelp(page);

    await page.getByRole("button", { name: "Details", exact: true }).click();
    await page.getByRole("heading", { name: "Game details" }).waitFor();
    await page.locator(".fields select").selectOption("ps1");
    await page.waitForTimeout(300);
    const stopped = await page
      .getByRole("button", { name: "Next", exact: true })
      .isDisabled();
    console.log(`PLAYSTATION next-disabled=${stopped}`);
    if (stopped) {
      await shot(page, path.join(out, "14-playstation-stopped.png"));
    }

    const seen = new Set(["megadrive"]);
    for (const profile of profiles()) {
      if (!profile.system || seen.has(profile.id)) continue;
      await page.getByRole("button", { name: "Details", exact: true }).click();
      await page.getByRole("heading", { name: "Game details" }).waitFor();
      await page.locator(".fields select").selectOption(profile.system);
      await page.waitForTimeout(200);
      const blocked = await page
        .getByRole("button", { name: "Next", exact: true })
        .isDisabled();
      if (blocked) {
        console.log(`UNREACHABLE ${profile.id} via ${profile.system}`);
        continue;
      }
      await clickNext(page);
      await page.getByRole("heading", { name: "Choose a menu" }).waitFor();
      await openControls(page);
      const sharing = profiles().filter((item) => item.system === profile.system);
      for (const item of sharing) {
        const variant = page.getByLabel("Controller variant");
        if ((await variant.count()) === 1) await variant.selectOption(item.id);
        await page.getByRole("table", { name: `${item.name} controls` }).waitFor();
        if (item.image) {
          await page.waitForFunction((stem) => {
            const image = document.querySelector("svg.controller-scene image");
            return (image?.getAttribute("href") || "").includes(stem);
          }, item.image.replace(/\.png$/, ""));
          const href = await page
            .locator("svg.controller-scene image")
            .getAttribute("href");
          console.log(`DRAWING ${item.id} ${href}`);
        } else {
          const scenes = await page.locator("svg.controller-scene").count();
          console.log(`DRAWING ${item.id} none scenes=${scenes}`);
        }
        await shootEditor(page, path.join(out, `controllers/${item.id}.png`));
        seen.add(item.id);
      }
    }
    for (const profile of profiles()) {
      if (!seen.has(profile.id)) console.log(`UNREACHABLE ${profile.id}`);
    }
  } finally {
    await browser.close();
    await new Promise((resolve) => server.close(resolve));
    // A return inside try skips everything after this block, so we exit here,
    // or a failed check would end with a success code.
    if (code !== 0) process.exit(code);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
