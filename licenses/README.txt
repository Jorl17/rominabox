The licence text of every third-party component ROM-in-a-Box uses or ships,
one file per component. scripts/licences.py writes this folder from each
component's own source; do not edit it by hand.

  native/      the player and what it links; a game carries those its player uses
  cores/       the libretro cores the builder downloads for an export
  crates/      the Rust crates the builder and its tools build with
  toolchains/  the Rust standard library the builder links
  npm/         the production packages of the builder's interface
  fonts/       the fonts of the menu designs and the builder
  data/        controller profiles, adapted artwork and catalogues

Each file names the component, the version the repository uses, where it
comes from, the licence it declares and what uses it, then each licence text,
headed by where it was read.

  uv run python scripts/licences.py            # write this folder again
  uv run python scripts/licences.py --check    # warn of a missing, stale or unused entry
