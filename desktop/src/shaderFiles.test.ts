import { describe, expect, it } from "vitest";
import { SHADER_ACCEPT, isShaderFile, shaderFileName } from "./shaderFiles";

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

  it("names a shader after its file", () => {
    expect(shaderFileName("/shaders/pal/pal-r57shell.slangp")).toBe(
      "pal-r57shell",
    );
    expect(shaderFileName("C:\\shaders\\crt.glsl")).toBe("crt");
    expect(shaderFileName(".slang")).toBe("Shader");
  });
});
