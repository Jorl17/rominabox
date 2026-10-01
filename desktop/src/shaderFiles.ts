// The shader files an author can add: GLSL and slang, each as a pass or a
// preset. In the exporter we read the contents of a file and reject anything
// else (shader_format.rs). Here we only limit the picker to those names.
export const SHADER_EXTENSIONS = ["glsl", "glslp", "slang", "slangp"];

const extension = new RegExp(`\\.(${SHADER_EXTENSIONS.join("|")})$`, "i");

function baseName(filePath: string): string {
  return filePath.split(/[\\/]/).pop() ?? "";
}

export function isShaderFile(filePath: string): boolean {
  return extension.test(baseName(filePath));
}

/** The names a shader file may end in, as an author reads them. */
export const SHADER_FORMATS = SHADER_EXTENSIONS.map((name) => `.${name}`);

export const SHADER_ACCEPT = SHADER_FORMATS.join(",");

export const NOT_A_SHADER_FILE = "Choose a GLSL or slang shader.";
