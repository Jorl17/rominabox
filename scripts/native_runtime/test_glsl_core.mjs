// The GLSL version for a core context, compiled from the function itself.
//
// Flycast requests an OpenGL 3.2 core context, and the macOS gl driver
// creates 4.1. The stock RetroArch shaders have no #version line, so
// RetroArch adds one in gl_glsl_compile_shader. A core profile rejects
// version 130, so a core context requires a later version. When a compile
// fails, we report the name of the shader, and stock shaders have a NULL path.
//
// We extract gl_glsl_core_version and gl_glsl_failed_shader_name from
// vendor/retroarch and run them. We stub path_basename so that a NULL call
// shows up as a failure instead of being hidden by the stub.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const native = resolve(root, 'vendor/retroarch');
const output = resolve(root, 'work/review-glsl-core');
mkdirSync(output, { recursive: true });

function extract(file, signature, nextSignature) {
  const source = readFileSync(resolve(native, file), 'utf8');
  const start = source.indexOf(signature);
  const end = source.indexOf(nextSignature, start);
  if (start < 0 || end < 0) throw new Error(`Missing function in ${file}: ${signature}`);
  return source.slice(start, end);
}

const version = extract(
  'gfx/drivers_shader/shader_glsl.c',
  'static unsigned gl_glsl_core_version(',
  '\n#endif',
);
const failed = extract(
  'gfx/drivers_shader/shader_glsl.c',
  'static const char *gl_glsl_failed_shader_name(',
  '\nstatic bool gl_glsl_compile_shader(',
);

const source = `
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdbool.h>

static int basename_calls;

static const char *path_basename(const char *path) {
   if (!path) {
      fprintf(stderr, "path_basename(NULL)\\n");
      abort();
   }
   ++basename_calls;
   return path;
}

${version}
${failed}

static int failures;
static void check(bool ok, const char *what) {
   if (ok) { printf("  ok   %s\\n", what); return; }
   printf("  FAIL %s\\n", what);
   ++failures;
}

int main(void) {
   unsigned got;

   got = gl_glsl_core_version(3, 2);
   check(got == 150, "a 3.2 core context gets GLSL 150, not 130");
   if (got != 150)
      printf("       3.2 core selected %u\\n", got);

   got = gl_glsl_core_version(3, 3);
   check(got == 330, "a 3.3 core context gets GLSL 330");
   if (got != 330)
      printf("       3.3 core selected %u\\n", got);

   got = gl_glsl_core_version(4, 1);
   check(got == 410, "a 4.1 core context gets GLSL 410");
   if (got != 410)
      printf("       4.1 core selected %u\\n", got);

   got = gl_glsl_core_version(4, 6);
   check(got == 460, "a 4.6 core context gets GLSL 460");
   if (got != 460)
      printf("       4.6 core selected %u\\n", got);

   /* Below a core profile we keep the old numbers: 2.1 is 120, older is 110. */
   check(gl_glsl_core_version(2, 1) == 120, "an OpenGL 2.1 context still gets GLSL 120");
   check(gl_glsl_core_version(2, 0) == 110, "an OpenGL 2.0 context still gets GLSL 110");

   basename_calls = 0;
   check(strcmp(gl_glsl_failed_shader_name(NULL), "stock") == 0,
         "a NULL shader path does not call path_basename");
   check(basename_calls == 0, "path_basename was not called for NULL");
   check(strcmp(gl_glsl_failed_shader_name(""), "stock") == 0,
         "an empty shader path does not call path_basename");
   check(strcmp(gl_glsl_failed_shader_name("/games/stock.glsl"), "/games/stock.glsl") == 0,
         "a real shader path is still handed to path_basename");

   if (failures) {
      printf("\\n%d GLSL core check(s) failed\\n", failures);
      return 1;
   }
   printf("\\na core context is given a GLSL version it accepts, and a missing path is not a basename\\n");
   return 0;
}
`;

const file = resolve(output, 'glsl-core.c');
const binary = resolve(output, 'glsl-core');
writeFileSync(file, source);
execFileSync('cc', ['-std=c11', '-Wall', '-Wextra', '-Werror', '-fsanitize=address',
  '-o', binary, file], { stdio: 'inherit' });
execFileSync(binary, { stdio: 'inherit' });
