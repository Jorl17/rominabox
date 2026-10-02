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
#   --hands-on-game GAME...  export the named games with the work/mac-build
#                         player into a new work/hands-on-<time> folder, with
#                         scripts/hands_on_game.py, and launch nothing
# For the scopes that run the player we build a separate test player, in
# work/test-player (scripts/player_build.py).
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

# One folder for the kit's player, so we compile only what changed.
player_build="$PWD/work/mac-build"

if [[ $# -eq 1 && "$1" == "--build-player" ]]; then
  kit_target=$(python3 -c 'import sys; sys.path.insert(0, "scripts"); import core_source, native_build; print(native_build.kit_target(core_source.host_target()))')
  print -u2 "run-tests: building $kit_target into $player_build"
  python3 scripts/build_player.py --target "$kit_target" "$player_build"
  exec python3 scripts/build_kit.py "$player_build"
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

for arg in "$@"; do
  if [[ ! "$arg" =~ '^(--list|--all|[a-z][a-z0-9-]*)$' ]]; then
    print -u2 "run-tests: refusing argument: $arg"
    exit 2
  fi
done

exec python3 scripts/test.py "$@"
