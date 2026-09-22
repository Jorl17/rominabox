#!/bin/sh
# Compile the menu-toggle decision with the test in which we hold one
# key and press another, then run it. The binary exits by itself.
set -eu
cd "$(dirname "$0")/.."
mkdir -p work/test-output
clang -Wall -Werror -I vendor/retroarch \
  -o work/test-output/test_held_key \
  vendor/retroarch/input/held_key_policy.c \
  scripts/native_runtime/test_held_key.c
work/test-output/test_held_key
clang -Wall -Werror -I vendor/retroarch \
  -o work/test-output/test_alt_enter \
  vendor/retroarch/input/alt_enter_fullscreen.c \
  scripts/native_runtime/test_alt_enter.c
work/test-output/test_alt_enter
