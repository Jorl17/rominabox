import { describe, expect, it } from "vitest";
import { SHADER_ACCEPT, isShaderFile } from "./shaderFiles";

describe("the shader files an author can add", () => {
  it("takes GLSL and slang, as a pass or a preset", () => {
    for (const name of [
      "crt.glsl",
      "crt.glslp",
      "pal.slang",
      "/a/b/PAL.SLANGP",
      "C:\\a\\pal.slangp",
    ]) {
      expect(isShaderFile(name)).toBe(true);
    }
    expect(SHADER_ACCEPT).toBe(".glsl,.glslp,.slang,.slangp");
  });

  it("leaves out what no exported game runs", () => {
    for (const name of [
      "old.cg",
      "old.cgp",
      "crt.slang.txt",
      "slang",
      "icon.png",
    ]) {
      expect(isShaderFile(name)).toBe(false);
    }
  });
});

describe("an added shader's name without the engine", () => {
  it("is the file's own name, whichever separator its path uses", async () => {
    const { customShaderName } = await import("./bridge");
    expect(await customShaderName("C:\\filters\\crt.glsl")).toBe("crt.glsl");
    expect(await customShaderName("/filters/pal.slangp")).toBe("pal.slangp");
  });
});
