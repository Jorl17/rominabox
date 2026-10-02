#!/bin/zsh
# Run scripts/test.py as from an interactive terminal and the pre-push hook,
# with plain python3, through pyenv when it is installed. Over SSH, a command
# runs without ~/.zprofile or ~/.zshrc, so without this script we would get
# Apple's Python 3.9 and no cargo, node or pyenv. The overlay digests depend
# on the Pillow version, so the interpreter must be the same one.
# Pass only scope names, --list or --all, or one of these alone:
#   --build-player        build the player we make the kit from (on a Mac
#                         the universal one) and its preview into
#                         work/mac-build with scripts/build_player.py, then
#                         make the kit and install the preview with
#                         scripts/build_kit.py. In a second build we compile
#                         only what changed.
#   --build-test-player   build a test-only player (menu script driver and
#                         achievements test host) into work/mac-test-build,
#                         again compiling only what changed
#   --hands-on-game GAME...  export the named games with the work/mac-build
#                         player into a new work/hands-on-<time> folder, with
#                         scripts/hands_on_game.py, and launch nothing
# or --with-test-build followed by scope names, to run those scopes against
# work/mac-test-build, as required for the native scopes.
emulate -L zsh
set -eu

if [[ -x /opt/homebrew/bin/brew ]]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
fi
if [[ -f ~/.cargo/env ]]; then
  source ~/.cargo/env
fi
if [[ -d ~/.local/bin ]]; then
  path=(~/.local/bin $path)
fi
if [[ -d ~/.pyenv/shims ]]; then
  path=(~/.pyenv/shims $path)
fi

cd "${0:A:h}/.."
print -u2 "run-tests: $(command -v python3) $(python3 --version 2>&1)"

# One folder per kind of build, so we compile only what changed in each.
player_build="$PWD/work/mac-build"
test_build="$PWD/work/mac-test-build"

if [[ $# -eq 1 && "$1" == "--build-player" ]]; then
  kit_target=$(python3 -c 'import sys; sys.path.insert(0, "scripts"); import core_source, native_build; print(native_build.kit_target(core_source.host_target()))')
  print -u2 "run-tests: building $kit_target into $player_build"
  python3 scripts/build_player.py --target "$kit_target" "$player_build"
  exec python3 scripts/build_kit.py "$player_build"
fi

if [[ $# -eq 1 && "$1" == "--build-test-player" ]]; then
  print -u2 "run-tests: building a test player into $test_build"
  export ROMINABOX_MENU_SCRIPT_BUILD=1 ROMINABOX_ACHIEVEMENTS_TEST_BUILD=1
  exec python3 scripts/build_player.py "$test_build"
fi

if [[ $# -eq 1 && "$1" == "--push-main" ]]; then
  # main to GitHub, fast-forward only, with the pre-push hook running under
  # the python3 above, as from an interactive terminal.
  exec git push origin main
fi

if [[ $# -ge 2 && "$1" == "--hands-on-game" ]]; then
  shift
  if [[ ! -d "$player_build" ]]; then
    print -u2 "run-tests: no $player_build; run --build-player first"
    exit 2
  fi
  output="$PWD/work/hands-on-$(date +%Y%m%d-%H%M%S)"
  print -u2 "run-tests: exporting with $player_build into $output"
  exec python3 scripts/hands_on_game.py "$player_build" "$output" "$@"
fi

with_test_build=0
if [[ $# -ge 2 && "$1" == "--with-test-build" ]]; then
  with_test_build=1
  shift
fi

for arg in "$@"; do
  if [[ ! "$arg" =~ '^(--list|--all|[a-z][a-z0-9-]*)$' ]]; then
    print -u2 "run-tests: refusing argument: $arg"
    exit 2
  fi
done

if [[ $with_test_build -eq 1 ]]; then
  if [[ ! -d "$test_build" ]]; then
    print -u2 "run-tests: no $test_build; run --build-test-player first"
    exit 2
  fi
  export ROMINABOX_TEST_BUILD="$test_build"
  export ROMINABOX_GAME_BUNDLE_PREFIX="app.rominabox.game.wt-native"
  print -u2 "run-tests: with test build $ROMINABOX_TEST_BUILD"
fi

exec python3 scripts/test.py "$@"
