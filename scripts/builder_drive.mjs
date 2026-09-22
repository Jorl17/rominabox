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

// The add control must not cover the cards, and no Bundle tick may repeat
// what is already on the card. We measure the grid as drawn, including 21
// and 22 cards, and reject a second way of showing "selected".
async function checkShaderPicker(page, out) {
  const problems = [];
  const notes = [];
  const initial = await page.evaluate(() => {
    const grid = document.querySelector(".shader-grid");
    const details = grid?.closest("details");
    const controls = document.querySelector("details.author-controls");
    const summaryOf = (element) =>
      (element?.querySelector("summary")?.innerText || "")
        .replace(/\s+/g, " ")
        .trim();
    const advanced = [...document.querySelectorAll("details")].find(
      (item) => summaryOf(item) === "Advanced",
    );
    const choices = document.querySelector(".shader-choices");
    const add = grid?.querySelector(":scope > .shader-add");
    const stray = [...document.querySelectorAll("button")].find((button) =>
      /add shader|add your own/i.test(button.textContent || ""),
    );
    let collision = false;
    if (!add && stray && grid) {
      const box = stray.getBoundingClientRect();
      collision = [...grid.querySelectorAll(".shader-card")].some((card) => {
        const other = card.getBoundingClientRect();
        return (
          box.left < other.right - 0.5 &&
          other.left < box.right - 0.5 &&
          box.top < other.bottom - 0.5 &&
          other.top < box.bottom - 0.5
        );
      });
    }
    const rules = [];
    for (const sheet of document.styleSheets) {
      let list;
      try {
        list = [...sheet.cssRules];
      } catch {
        continue;
      }
      const walk = (entries) => {
        for (const rule of entries) {
          if (rule.cssRules) walk([...rule.cssRules]);
          if (rule.type !== CSSRule.STYLE_RULE || !rule.selectorText) continue;
          const selectors = rule.selectorText.split(",").map((part) => part.trim());
          if (selectors.includes(".shader-card.chosen")) rules.push(rule.cssText);
        }
      };
      walk(list);
    }
    return {
      summary: summaryOf(details),
      followsControls: !!(
        controls &&
        details &&
        controls.compareDocumentPosition(details) &
          Node.DOCUMENT_POSITION_FOLLOWING
      ),
      gridInAdvanced: !!advanced?.querySelector(".shader-grid"),
      bundle: /\bBundle\b/.test(choices?.innerText || ""),
      checkboxes: choices?.querySelectorAll("input[type='checkbox']").length || 0,
      addText: (add?.innerText || "").replace(/\s+/g, " ").trim(),
      stray: stray ? (stray.textContent || "").replace(/\s+/g, " ").trim() : "",
      collision,
      rules,
      highlight: getComputedStyle(document.documentElement)
        .getPropertyValue("--highlight")
        .trim(),
    };
  });

  if (initial.summary !== "Picture filters · none selected") {
    problems.push(
      `picture filters are not their own section titled with the count (summary is ${JSON.stringify(initial.summary)})`,
    );
  }
  if (!initial.followsControls) {
    problems.push("picture filters are not below Controls");
  }
  if (initial.gridInAdvanced) {
    problems.push("the shader catalogue is still inside Advanced");
  }
  if (initial.bundle || initial.checkboxes) {
    problems.push(
      `selecting a filter is said twice: the card and a Bundle control (bundle=${initial.bundle} checkboxes=${initial.checkboxes})`,
    );
  }
  if (!initial.addText.includes("Add your own")) {
    problems.push(
      `Add your own is not a card in the shader grid` +
        (initial.stray ? ` (found ${JSON.stringify(initial.stray)} outside it` : "") +
        (initial.collision ? ", and it overlaps a shader card)" : initial.stray ? ")" : ""),
    );
  }
  const chosenRule = initial.rules.join("\n");
  const usesHighlight = /background(?:-color)?:\s*var\(--highlight\)/.test(
    chosenRule,
  );
  const paintsHex = /background(?:-color)?:\s*[^;]*#[0-9a-fA-F]{3,8}/.test(
    chosenRule,
  );
  if (!initial.highlight || !usesHighlight || paintsHex) {
    problems.push(
      `the selected card's background is not the theme highlight (token=${JSON.stringify(initial.highlight)} rule=${JSON.stringify(chosenRule)})`,
    );
  }

  if (out && initial.summary.startsWith("Picture filters")) {
    const section = page.locator(".picture-filters");
    await section.scrollIntoViewIfNeeded();
    await section.screenshot({
      path: path.join(out, "10-shaders-none.png"),
      animations: "disabled",
    });
    notes.push(`shot ${path.relative(ROOT, path.join(out, "10-shaders-none.png"))}`);
  }

  // We use a DOM click, not a pointer click. With a pointer click we would
  // wait until nothing covers the card, so an overlapping Add card would end
  // in a timeout instead of the overlap error from the grid check below.
  await page.evaluate(() => {
    document
      .querySelector(".shader-grid > .shader-card:not(.shader-add)")
      ?.click();
  });
  const one = await page.evaluate(() => {
    const details = document.querySelector(".shader-grid")?.closest("details");
    return (details?.querySelector("summary")?.innerText || "")
      .replace(/\s+/g, " ")
      .trim();
  });
  if (one !== "Picture filters · 1 selected") {
    problems.push(
      `selecting one filter left the title as ${JSON.stringify(one)}`,
    );
  }
  await page.evaluate(() => {
    document
      .querySelectorAll(".shader-grid > .shader-card:not(.shader-add)")[1]
      ?.click();
  });
  const wash = await page.evaluate(() => {
    const card = document.querySelector(".shader-card.chosen");
    const probe = document.createElement("div");
    probe.style.backgroundColor = "var(--highlight)";
    document.body.appendChild(probe);
    const expected = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return {
      title: (
        document.querySelector(".shader-grid")?.closest("details")?.querySelector("summary")
          ?.innerText || ""
      )
        .replace(/\s+/g, " ")
        .trim(),
      expected,
      actual: card ? getComputedStyle(card).backgroundColor : "(no chosen card)",
      checkbox: !!card?.querySelector("input"),
    };
  });
  if (wash.title !== "Picture filters · 2 selected") {
    problems.push(
      `selecting two filters left the title as ${JSON.stringify(wash.title)}`,
    );
  }
  if (wash.checkbox) {
    problems.push("the selected card still contains a checkbox");
  }
  if (wash.actual !== wash.expected) {
    problems.push(
      `selected background is ${wash.actual}; the theme highlight is ${wash.expected}`,
    );
  }

  const layout = await page.evaluate(() => {
    const grid = document.querySelector(".shader-grid");
    const add = grid?.querySelector(":scope > .shader-add");
    if (!grid || !add) {
      return {
        error:
          "Add your own is not a card in the shader grid, so 1, 2, 21 and 22 shaders were not measured",
      };
    }
    const originals = [
      ...grid.querySelectorAll(":scope > .shader-card:not(.shader-add)"),
    ];
    if (!originals.length) {
      return { error: "the shader grid has no filter cards to measure" };
    }
    const clones = [];
    const results = [];
    const overlaps = (a, b) =>
      a.left < b.right - 0.5 &&
      b.left < a.right - 0.5 &&
      a.top < b.bottom - 0.5 &&
      b.top < a.bottom - 0.5;
    try {
      for (const n of [1, 2, 21, 22]) {
        while (originals.length + clones.length < n) {
          const clone = originals[0].cloneNode(true);
          clone.setAttribute("data-clone", "");
          grid.insertBefore(clone, add);
          clones.push(clone);
        }
        [...originals, ...clones].forEach((element, index) => {
          element.hidden = index >= n;
        });
        add.hidden = false;
        add.scrollIntoView({ block: "center", inline: "nearest" });
        const visible = [
          ...grid.querySelectorAll(":scope > .shader-card"),
        ].filter((element) => !element.hidden);
        const rects = visible.map((element) => {
          const box = element.getBoundingClientRect();
          return {
            left: box.left,
            top: box.top,
            right: box.right,
            bottom: box.bottom,
            add: element === add,
            text: (element.innerText || "").replace(/\s+/g, " ").trim().slice(0, 40),
          };
        });
        const hits = [];
        for (let i = 0; i < rects.length; i += 1) {
          for (let j = i + 1; j < rects.length; j += 1) {
            if (overlaps(rects[i], rects[j])) {
              hits.push(`${rects[i].text} ∩ ${rects[j].text}`);
            }
          }
        }
        const addRect = rects.find((rect) => rect.add);
        const area = addRect
          ? (addRect.right - addRect.left) * (addRect.bottom - addRect.top)
          : 0;
        const hit = addRect
          ? document.elementFromPoint(
              (addRect.left + addRect.right) / 2,
              (addRect.top + addRect.bottom) / 2,
            )
          : null;
        const rowTops = [...new Set(rects.map((rect) => Math.round(rect.top)))].sort(
          (a, b) => a - b,
        );
        results.push({
          n,
          shaders: rects.filter((rect) => !rect.add).length,
          overlaps: hits,
          reachable: !!(
            addRect &&
            area > 100 &&
            hit &&
            (hit === add || add.contains(hit))
          ),
          last: visible[visible.length - 1] === add,
          rows: rowTops.length,
          addRow: addRect ? rowTops.indexOf(Math.round(addRect.top)) + 1 : 0,
        });
      }
    } finally {
      clones.forEach((clone) => clone.remove());
      originals.forEach((element) => {
        element.hidden = false;
      });
    }
    return { results };
  });
  if (layout.error) {
    problems.push(layout.error);
  } else {
    for (const row of layout.results) {
      notes.push(
        `GRID ${row.n} shaders rows=${row.rows} addRow=${row.addRow} reachable=${row.reachable} overlaps=${row.overlaps.length}`,
      );
      if (row.shaders !== row.n || !row.reachable || !row.last || row.overlaps.length) {
        problems.push(`shader grid at ${row.n}: ${JSON.stringify(row)}`);
      }
    }
  }

  if (!problems.length) {
    const dropped = await page.evaluate(() => {
      const zone = document.querySelector(".shader-grid > .shader-add");
      if (!zone) return false;
      const file = new File(["void main(){}"], "crt-own.glsl");
      const data = new DataTransfer();
      data.items.add(file);
      zone.dispatchEvent(
        new DragEvent("drop", {
          bubbles: true,
          cancelable: true,
          dataTransfer: data,
        }),
      );
      return true;
    });
    if (!dropped) {
      problems.push("dropping a .glsl file on Add your own did not add it");
    } else {
      try {
        await page.waitForFunction(
          () =>
            (document.querySelector(".shader-grid")?.textContent || "").includes(
              "crt-own",
            ),
          { timeout: 2000 },
        );
      } catch {
        problems.push("dropping a .glsl file on Add your own did not add it");
      }
      const afterDrop = await page.evaluate(() =>
        (document.querySelector(".picture-filters summary")?.innerText || "")
          .replace(/\s+/g, " ")
          .trim(),
      );
      if (afterDrop !== "Picture filters · 3 selected") {
        problems.push(
          `adding a shader by drop left the title as ${JSON.stringify(afterDrop)}`,
        );
      }
      const custom = page.locator(".shader-grid .shader-card", {
        hasText: "crt-own",
      });
      if (await custom.count()) await custom.first().click();
    }
  }

  return { problems, notes };
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
  await editor.evaluate((element) =>
    element.scrollIntoView({ block: "start" }),
  );
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
    await page
      .getByRole("heading", { name: "SONIC THE HEDGEHOG.md" })
      .waitFor();
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
    console.log(
      `IDENTIFIED title=${JSON.stringify(identified)} system=${system}`,
    );
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
    const designs = await page
      .getByLabel("Menu design")
      .evaluate((select) =>
        [...select.options].map((option) => option.textContent.trim()),
      );
    console.log(`DESIGNS ${designs.join(" | ")}`);

    // Every label on a step must be in the interface font, including the
    // design selector and the palette names. We read the font from the
    // browser, not from the stylesheet, because the result of the cascade is
    // what appears on screen. We run this check on the --check path, which
    // is the one in the test suite.
    const fonts = await page.evaluate(() => {
      const family = (selector) => {
        const element = document.querySelector(selector);
        return element ? getComputedStyle(element).fontFamily : "(absent)";
      };
      return {
        body: getComputedStyle(document.body).fontFamily,
        designLabel: family(".design-select"),
        paletteButton: family(".palette-picker button"),
      };
    });
    console.log(`FONTS ${JSON.stringify(fonts)}`);
    const odd = ["designLabel", "paletteButton"].filter(
      (key) => fonts[key] !== fonts.body,
    );
    if (odd.length) {
      console.error("a control on the menu step is not in the interface font");
      console.error(
        odd.map((key) => `${key} is ${fonts[key]}`).join("; ") +
          `; body is ${fonts.body}`,
      );
      code = 1;
      return;
    }

    if (!checking) {
      for (const palette of ["blue", "green", "amber"]) {
        await page
          .getByRole("button", {
            name: palette[0].toUpperCase() + palette.slice(1),
          })
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
    // Opening picture filters must show the cards. On the Menu step the
    // section is below a 445pt preview inside a scrolling content area, so
    // opening it must scroll it into view.
    // The scroll happens on the frame after the section opens, so we wait
    // for it to settle instead of racing it.
    await page.waitForTimeout(400);
    const visible = await page.evaluate(() => {
      const details = [...document.querySelectorAll("details.advanced")].find(
        (element) => element.open && element.querySelector(".shader-choices"),
      );
      if (!details) return { found: false };
      const screen = details.closest(".screen");
      // We check the cards, not the summary row. The check must fail when the
      // top of the summary is just inside the area and every shader is below
      // the fold, and the <details> element is the same in both cases.
      const box = details.querySelector(".shader-grid").getBoundingClientRect();
      const frame = screen
        ? screen.getBoundingClientRect()
        : { top: 0, bottom: window.innerHeight };
      return {
        found: true,
        top: Math.round(box.top),
        frameTop: Math.round(frame.top),
        frameBottom: Math.round(frame.bottom),
        bottom: Math.round(box.bottom),
        inside: box.top >= frame.top - 1 && box.bottom <= frame.bottom + 1,
      };
    });
    console.log(`ADVANCED ${JSON.stringify(visible)}`);
    if (!visible.found || !visible.inside) {
      console.error(
        "opening the picture filters does not bring the cards into view",
      );
      console.error(JSON.stringify(visible));
      code = 1;
      return;
    }

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
    const picker = await checkShaderPicker(page, checking ? null : out);
    for (const line of picker.notes) console.log(line);
    if (picker.problems.length) {
      for (const problem of picker.problems) console.error(problem);
      code = 1;
      return;
    }
    if (!checking) {
      await shot(page, path.join(out, "09-shaders.png"));
      await page.getByLabel("Starts on").selectOption("phosphor");
      const shadersRow = page.locator(".picture-filters");
      await shadersRow.scrollIntoViewIfNeeded();
      await shot(page, path.join(out, "10-shaders-chosen.png"));
      await shadersRow.screenshot({
        path: path.join(out, "10-shaders-row.png"),
        animations: "disabled",
      });
      console.log(
        `shot ${path.relative(ROOT, path.join(out, "10-shaders-row.png"))}`,
      );
      // We also check the grid at 21 cards, which is not a round number. The
      // catalogue has two presets, so we add copies, labelled as copies, and
      // remove them before the walk continues.
      await page.evaluate(() => {
        const grid = document.querySelector(".shader-grid");
        const add = grid.querySelector(".shader-add");
        const original = grid.querySelector(".shader-card:not(.shader-add)");
        const clones = [];
        while (
          grid.querySelectorAll(".shader-card:not(.shader-add)").length < 21
        ) {
          const clone = original.cloneNode(true);
          clone.setAttribute("data-clone", "");
          const name = clone.querySelector(".shader-name");
          if (name) name.textContent = `Filter ${clones.length + 3}`;
          grid.insertBefore(clone, add);
          clones.push(clone);
        }
        window.__shaderClones = clones;
      });
      // The content of the menu step scrolls inside .screen. A shot of an
      // element in there stops at the fold, so a 21-card grid looks like two
      // rows and then the Back button. We lift the clip for the shot, then put it back.
      try {
        await page.evaluate(() => {
          for (const selector of [".app-shell", "main", ".screen"]) {
            const element = document.querySelector(selector);
            element.dataset.shotHeight = element.style.height;
            element.dataset.shotOverflow = element.style.overflow;
            element.style.height = "auto";
            element.style.overflow = "visible";
          }
        });
        const gridBox = await page.locator(".shader-grid").boundingBox();
        if (!gridBox) throw new Error("the shader grid has no box to photograph");
        await page.setViewportSize({
          width: 1440,
          height: Math.min(4800, Math.ceil(gridBox.y + gridBox.height + 24)),
        });
        await page.locator(".shader-grid").screenshot({
          path: path.join(out, "10-shaders-21.png"),
          animations: "disabled",
        });
        console.log(
          `shot ${path.relative(ROOT, path.join(out, "10-shaders-21.png"))}`,
        );
      } finally {
        await page.evaluate(() => {
          for (const clone of window.__shaderClones || []) clone.remove();
          delete window.__shaderClones;
          for (const selector of [".app-shell", "main", ".screen"]) {
            const element = document.querySelector(selector);
            if (!element || element.dataset.shotHeight === undefined) continue;
            element.style.height = element.dataset.shotHeight;
            element.style.overflow = element.dataset.shotOverflow;
          }
        });
        await page.setViewportSize({ width: 1440, height: 900 });
      }
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
      const create = page.getByRole("button", {
        name: "Create app",
        exact: true,
      });
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
      const sharing = profiles().filter(
        (item) => item.system === profile.system,
      );
      for (const item of sharing) {
        const variant = page.getByLabel("Controller variant");
        if ((await variant.count()) === 1) await variant.selectOption(item.id);
        await page
          .getByRole("table", { name: `${item.name} controls` })
          .waitFor();
        if (item.image) {
          await page.waitForFunction(
            (stem) => {
              const image = document.querySelector(
                "svg.controller-scene image",
              );
              return (image?.getAttribute("href") || "").includes(stem);
            },
            item.image.replace(/\.png$/, ""),
          );
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
