/**
 * Render each shader preset's preview by running the shader, then exit.
 *
 * We make the picture of a filter by applying the filter, so every preset in
 * the catalogue has a preview, and a preview changes whenever a change to its
 * fragment changes the game.
 *
 * We take the GLSL from the exporter, through `rominabox-cli shader-sources`,
 * so we compile exactly the GLSL of an exported game. As in the RetroArch GL
 * driver, we compile one source twice, with VERTEX and then FRAGMENT defined.
 * WebGL is GLSL ES, which is the `GL_ES` branch that those presets already
 * have for mobile.
 *
 * We use the browser that is already installed, download nothing, and close
 * the browser before we return.
 */
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { findChrome } from "./chrome.mjs";

const require = createRequire(
  new URL("../desktop/package.json", import.meta.url),
);
const { chromium } = require("playwright-core");

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function argument(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? null : process.argv[index + 1];
}

/* Run in the page. We pass in everything it uses, because the page does not
 * share the scope of this file. */
function renderInPage({ shaders, width, height }) {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const gl = canvas.getContext("webgl", {
    antialias: false,
    preserveDrawingBuffer: true,
    premultipliedAlpha: false,
  });
  if (!gl) throw new Error("this browser gave no WebGL context");

  /* The picture we apply the filter to. It looks like a console's picture,
   * not a photograph, with flat blocks of colour, a ramp and one-pixel detail,
   * which are what these filters change. We draw it from numbers, so it is the
   * same on every machine and is not an asset that anyone has to keep. */
  const CARD = 64;
  const card = new Uint8Array(CARD * CARD * 4);
  const bars = [
    [232, 232, 232],
    [232, 232, 32],
    [32, 232, 232],
    [32, 200, 48],
    [232, 48, 200],
    [216, 40, 40],
    [48, 64, 216],
    [16, 16, 16],
  ];
  for (let y = 0; y < CARD; y += 1) {
    for (let x = 0; x < CARD; x += 1) {
      let colour;
      if (y < CARD * 0.55) {
        colour = bars[Math.floor((x / CARD) * bars.length)];
      } else if (y < CARD * 0.78) {
        const ramp = Math.round((x / (CARD - 1)) * 255);
        colour = [ramp, ramp, ramp];
      } else {
        // One-pixel checks and a block like a sprite, so that the effect of a
        // scanline filter is visible.
        const check = (x + y) % 2 === 0 ? 236 : 24;
        const block = x > CARD * 0.62 && x < CARD * 0.86 && y > CARD * 0.83;
        colour = block ? [244, 176, 32] : [check, check, check];
      }
      const at = (y * CARD + x) * 4;
      card[at] = colour[0];
      card[at + 1] = colour[1];
      card[at + 2] = colour[2];
      card[at + 3] = 255;
    }
  }

  const texture = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texImage2D(
    gl.TEXTURE_2D,
    0,
    gl.RGBA,
    CARD,
    CARD,
    0,
    gl.RGBA,
    gl.UNSIGNED_BYTE,
    card,
  );
  // Nearest, because a console's picture is not smoothed, and in `preset_text`
  // we write filter_linear0 = false.
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.bufferData(
    gl.ARRAY_BUFFER,
    // x, y, u, v, with v downward so that the card is not upside down.
    new Float32Array([
      -1, -1, 0, 1, 1, -1, 1, 1, -1, 1, 0, 0, 1, 1, 1, 0,
    ]),
    gl.STATIC_DRAW,
  );

  function build(source, kind, defineName) {
    const shader = gl.createShader(kind);
    gl.shaderSource(shader, `#define ${defineName}\n${source}`);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      throw new Error(`${defineName}: ${gl.getShaderInfoLog(shader)}`);
    }
    return shader;
  }

  const results = [];
  for (const entry of shaders) {
    const program = gl.createProgram();
    gl.attachShader(program, build(entry.glsl, gl.VERTEX_SHADER, "VERTEX"));
    gl.attachShader(program, build(entry.glsl, gl.FRAGMENT_SHADER, "FRAGMENT"));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(`${entry.id}: ${gl.getProgramInfoLog(program)}`);
    }
    gl.useProgram(program);

    const vertexCoord = gl.getAttribLocation(program, "VertexCoord");
    const texCoord = gl.getAttribLocation(program, "TexCoord");
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    if (vertexCoord >= 0) {
      gl.enableVertexAttribArray(vertexCoord);
      gl.vertexAttribPointer(vertexCoord, 2, gl.FLOAT, false, 16, 0);
    }
    if (texCoord >= 0) {
      gl.enableVertexAttribArray(texCoord);
      gl.vertexAttribPointer(texCoord, 2, gl.FLOAT, false, 16, 8);
    }
    const colour = gl.getAttribLocation(program, "COLOR");
    if (colour >= 0) gl.vertexAttrib4f(colour, 1, 1, 1, 1);

    // The same uniforms as in RetroArch. The line count of a scanline preset
    // comes from TextureSize, so it is the picture's size, not the window's.
    const matrix = gl.getUniformLocation(program, "MVPMatrix");
    if (matrix) {
      gl.uniformMatrix4fv(matrix, false, [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
    }
    const setVec2 = (name, x, y) => {
      const where = gl.getUniformLocation(program, name);
      if (where) gl.uniform2f(where, x, y);
    };
    setVec2("TextureSize", CARD, CARD);
    setVec2("InputSize", CARD, CARD);
    setVec2("OutputSize", width, height);
    const setInt = (name, value) => {
      const where = gl.getUniformLocation(program, name);
      if (where) gl.uniform1i(where, value);
    };
    setInt("FrameCount", 1);
    setInt("FrameDirection", 1);
    setInt("Texture", 0);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, texture);

    gl.viewport(0, 0, width, height);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    const error = gl.getError();
    if (error !== gl.NO_ERROR) {
      throw new Error(`${entry.id}: GL error ${error}`);
    }
    results.push({ id: entry.id, png: canvas.toDataURL("image/png") });
  }
  return results;
}

async function main() {
  const out = argument("--out");
  const input = argument("--shaders");
  if (!out || !input) {
    throw new Error("usage: shader_previews.mjs --shaders FILE --out DIR");
  }
  const shaders = JSON.parse(fs.readFileSync(input, "utf8"));
  // Square, because the space for a picture in a list row is square, and a
  // 4:3 preview stretched into it would misrepresent the filter. The card
  // itself is square, so nothing is distorted.
  const width = Number(argument("--width") || 256);
  const height = Number(argument("--height") || 256);

  const browser = await chromium.launch({
    executablePath: findChrome(),
    headless: true,
    // In headless Chrome, WebGL runs on a software rasteriser, so we get the
    // same picture on a machine with any GPU, or none.
    args: ["--use-gl=swiftshader", "--enable-unsafe-swiftshader"],
  });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(30000);
    await page.goto("about:blank");
    const rendered = await page.evaluate(renderInPage, {
      shaders,
      width,
      height,
    });
    fs.mkdirSync(out, { recursive: true });
    for (const entry of rendered) {
      const bytes = Buffer.from(entry.png.split(",")[1], "base64");
      fs.writeFileSync(path.join(out, `${entry.id}.png`), bytes);
      process.stdout.write(`  drew    ${entry.id}.png\n`);
    }
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  process.stderr.write(`${error?.message || error}\n`);
  process.exitCode = 1;
});

void ROOT;
