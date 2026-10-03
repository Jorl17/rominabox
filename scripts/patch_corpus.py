"""Patches with known results, to check patching in the engine against.

Each case is a patch, the game it starts from and the result it must give. We
do not store the game: we generate it from a seed and a size (splitmix64,
which `desktop/crates/rominabox-engine/tests/patch_corpus.rs` implements the
same way). In the engine test we apply every patch through the same path as
export and compare the result, or expect an error for a patch of another game.

The result that a patch must give:

- xdelta: what the xdelta3 command decodes from it (`xdelta3 -d`), the tool
  of the format, which must also be the target that the case was made from.
- IPS, UPS and BPS: the target that the case was made from. We write these
  patches here from the published layouts of the formats (BPS with all four
  of its commands). Homebrew has no reference patcher for them, so the
  target is the reference.

  uv run python scripts/patch_corpus.py fixtures
      rewrites the committed corpus (tests/fixtures/patches/corpus), the edge
      cases of each format. Requires xdelta3.
  uv run python scripts/patch_corpus.py crosscheck [--cases N] [--seed S]
      makes N random cases of each format in work/patch-corpus, checks them
      all with the engine's corpus test, and removes the folder. Requires
      xdelta3 and cargo. Not in any test scope, as Windows has no xdelta3.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ENGINE = ROOT / "desktop/crates/rominabox-engine"
FIXTURES = ENGINE / "tests/fixtures/patches/corpus"
CROSSCHECK = ROOT / "work/patch-corpus"
MASK = (1 << 64) - 1
WINDOW = 16384


def source_bytes(seed: int, size: int) -> bytes:
    """splitmix64's output, eight little-endian bytes a step."""
    out = bytearray()
    state = seed & MASK
    while len(out) < size:
        state = (state + 0x9E3779B97F4A7C15) & MASK
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        z ^= z >> 31
        out += z.to_bytes(8, "little")
    return bytes(out[:size])


# --- the formats' writers ---------------------------------------------------


def number(value: int) -> bytes:
    """byuu's variable-length number, as UPS and BPS write it."""
    out = bytearray()
    while True:
        low = value & 0x7F
        value >>= 7
        if value == 0:
            out.append(0x80 | low)
            return bytes(out)
        out.append(low)
        value -= 1


def crc(data: bytes) -> bytes:
    return (zlib.crc32(data) & 0xFFFFFFFF).to_bytes(4, "little")


def ips(source: bytes, target: bytes) -> bytes:
    """Records for each run of bytes that differ, a run of one value as RLE,
    and the truncation extension when the target is shorter."""
    patch = bytearray(b"PATCH")
    offset = 0
    while offset < len(target):
        if offset < len(source) and source[offset] == target[offset]:
            offset += 1
            continue
        start = offset
        while (
            offset < len(target)
            and not (offset < len(source) and source[offset] == target[offset])
            and offset - start < 0xFFFF
        ):
            offset += 1
        run = target[start:offset]
        assert start < 0x1000000 and start != 0x454F46
        patch += start.to_bytes(3, "big")
        if len(run) >= 4 and run.count(run[0]) == len(run):
            patch += b"\x00\x00" + len(run).to_bytes(2, "big") + run[:1]
        else:
            patch += len(run).to_bytes(2, "big") + run
    patch += b"EOF"
    if len(target) < len(source):
        patch += len(target).to_bytes(3, "big")
    return bytes(patch)


def ups(source: bytes, target: bytes) -> bytes:
    """Runs of bytes to skip, each followed by the differing bytes XORed with
    the source's and a zero, then the three CRC-32s."""
    patch = bytearray(b"UPS1") + number(len(source)) + number(len(target))
    length = max(len(source), len(target))
    at = lambda data, i: data[i] if i < len(data) else 0  # noqa: E731
    offset = relative = 0
    while offset < length:
        if at(source, offset) == at(target, offset):
            offset += 1
            continue
        patch += number(offset - relative)
        while offset < length and at(source, offset) != at(target, offset):
            patch.append(at(source, offset) ^ at(target, offset))
            offset += 1
        patch.append(0)
        offset += 1
        relative = offset
    patch += crc(source) + crc(target)
    return bytes(patch + crc(bytes(patch)))


def grams(data: bytes, size: int = 4) -> dict[bytes, list[int]]:
    found: dict[bytes, list[int]] = {}
    for at in range(len(data) - size + 1):
        found.setdefault(data[at : at + size], []).append(at)
    return found


def matching(a: bytes, a_at: int, b: bytes, b_at: int) -> int:
    length = 0
    while a_at + length < len(a) and b_at + length < len(b) and a[a_at + length] == b[b_at + length]:
        length += 1
    return length


def bps(source: bytes, target: bytes, rng: random.Random) -> bytes:
    """Greedy, with every command: SourceRead where the target matches the
    source in place, SourceCopy and TargetCopy (overlapping its own output
    when that matches) from anywhere, TargetRead for the rest."""
    patch = bytearray(b"BPS1") + number(len(source)) + number(len(target)) + number(0)
    source_grams = grams(source)
    target_grams: dict[bytes, list[int]] = {}
    literal = bytearray()
    source_at = target_at = 0
    indexed = 0

    def command(mode: int, length: int) -> None:
        patch.extend(number(((length - 1) << 2) | mode))

    def offset(delta: int) -> None:
        patch.extend(number((abs(delta) << 1) | (1 if delta < 0 else 0)))

    def flush() -> None:
        if literal:
            command(1, len(literal))
            patch.extend(literal)
            literal.clear()

    i = 0
    while i < len(target):
        while indexed + 4 <= i:
            target_grams.setdefault(target[indexed : indexed + 4], []).append(indexed)
            indexed += 1
        in_place = matching(source, i, target, i)
        if in_place >= 4:
            flush()
            command(0, in_place)
            i += in_place
            continue
        gram = target[i : i + 4]
        options = [(matching(source, at, target, i), 2, at) for at in source_grams.get(gram, [])[-8:]]
        options += [(matching(target, at, target, i), 3, at) for at in target_grams.get(gram, [])[-8:]]
        options = [option for option in options if option[0] >= 4]
        if options:
            longest = max(option[0] for option in options)
            length, mode, at = rng.choice([option for option in options if option[0] == longest])
            flush()
            command(mode, length)
            if mode == 2:
                offset(at - source_at)
                source_at = at + length
            else:
                offset(at - target_at)
                target_at = at + length
            i += length
            continue
        literal.append(target[i])
        i += 1
    flush()
    patch += crc(source) + crc(target)
    return bytes(patch + crc(bytes(patch)))


def xdelta(source: bytes, target: bytes, options: list[str], folder: Path) -> bytes:
    """The output of xdelta3, checked by decoding it with xdelta3."""
    source_file, target_file = folder / "source.bin", folder / "target.bin"
    patch_file, decoded_file = folder / "patch.xdelta", folder / "decoded.bin"
    source_file.write_bytes(source)
    target_file.write_bytes(target)
    subprocess.run(
        ["xdelta3", "-e", "-f", *options, "-s", source_file.name, target_file.name, patch_file.name],
        cwd=folder,
        check=True,
    )
    subprocess.run(
        ["xdelta3", "-d", "-f", "-s", source_file.name, patch_file.name, decoded_file.name],
        cwd=folder,
        check=True,
    )
    if decoded_file.read_bytes() != target:
        raise SystemExit("xdelta3 does not decode its own patch to the target it was made from")
    return patch_file.read_bytes()


# --- the cases --------------------------------------------------------------


def edited(source: bytes, rng: random.Random, edits: int) -> bytes:
    """`source` with random overwrites, insertions, deletions, fills and
    repeats of its own parts."""
    target = bytearray(source)
    for _ in range(edits):
        at = rng.randint(0, len(target))
        length = rng.randint(1, 300)
        kind = rng.choice(["overwrite", "insert", "delete", "fill", "repeat"])
        if kind == "overwrite":
            target[at : at + length] = rng.randbytes(length)
        elif kind == "insert":
            target[at:at] = rng.randbytes(length)
        elif kind == "delete":
            del target[at : at + length]
        elif kind == "fill":
            target[at : at + length] = bytes([rng.randrange(256)]) * length
        else:
            start = rng.randint(0, len(target))
            target[at:at] = target[start : start + length]
    return bytes(target)


def changed_at(source: bytes, at: int) -> bytes:
    target = bytearray(source)
    target[at] ^= 0x5A
    return bytes(target)


XDELTA_KINDS = {
    "xdelta": [],
    "xdelta-plain": ["-S", "none", "-n"],
}


class Corpus:
    def __init__(self, folder: Path, rng: random.Random):
        self.folder = folder
        self.rng = rng
        self.cases: list[dict] = []
        self.work = Path(tempfile.mkdtemp(prefix="patch-corpus-", dir=folder))

    def add(self, name: str, kind: str, seed: int, size: int, target: bytes, *, options=(), wrong_at=None):
        source = source_bytes(seed, size)
        if kind == "ips":
            patch, extension = ips(source, target), "ips"
        elif kind == "ups":
            patch, extension = ups(source, target), "ups"
        elif kind == "bps":
            patch, extension = bps(source, target, self.rng), "bps"
        else:
            patch, extension = xdelta(source, target, [*XDELTA_KINDS[kind], *options], self.work), "xdelta"
        file_name = f"{name}.{extension}"
        (self.folder / file_name).write_bytes(patch)
        case = {"patch": file_name, "seed": seed, "size": size}
        if wrong_at is None:
            case["target"] = {"size": len(target), "sha256": hashlib.sha256(target).hexdigest()}
        else:
            case["flip"] = wrong_at
            case["refused"] = True
        self.cases.append(case)

    def finish(self) -> None:
        manifest = {"about": "Written by scripts/patch_corpus.py; read by tests/patch_corpus.rs.", "cases": self.cases}
        (self.folder / "corpus.json").write_text(json.dumps(manifest, indent=1) + "\n")
        for name in ("source.bin", "target.bin", "patch.xdelta", "decoded.bin"):
            if (self.work / name).is_file():
                (self.work / name).unlink()
        self.work.rmdir()


KINDS = ["ips", "ups", "bps", *XDELTA_KINDS]
# The formats with a check of the game they start from. IPS has none, and
# xdelta made with -n has no checksums.
CHECKED = ["ups", "bps", "xdelta"]


def edge_cases(corpus: Corpus) -> None:
    """The edges each format has, for every format."""
    for kind in KINDS:
        seed = 1000 + KINDS.index(kind) * 100
        source = source_bytes(seed, 6000)
        corpus.add(f"{kind}-first-byte", kind, seed, 6000, changed_at(source, 0))
        corpus.add(f"{kind}-last-byte", kind, seed, 6000, changed_at(source, 5999))
        corpus.add(f"{kind}-larger", kind, seed, 6000, edited(source, corpus.rng, 3) + corpus.rng.randbytes(3000))
        corpus.add(f"{kind}-smaller", kind, seed, 6000, edited(source, corpus.rng, 3)[:4000])
        corpus.add(f"{kind}-unchanged", kind, seed, 6000, source)
        corpus.add(f"{kind}-from-nothing", kind, seed, 0, corpus.rng.randbytes(500))
        for index in range(4):
            corpus.add(f"{kind}-random-{index}", kind, seed + 1 + index, 2000 + 1500 * index,
                       edited(source_bytes(seed + 1 + index, 2000 + 1500 * index), corpus.rng, 2 + 3 * index))
        if kind in CHECKED:
            # Made for this game, and applied to it with one copied byte
            # changed, as for another revision of the game.
            corpus.add(f"{kind}-another-game", kind, seed, 6000, changed_at(source, 5999), wrong_at=3000)
    for kind in XDELTA_KINDS:
        # Windows of 16 KiB: changes on and across their boundaries, and a
        # target whose windows no longer line up with the source's.
        seed = 5000 + list(XDELTA_KINDS).index(kind)
        source = source_bytes(seed, 4 * WINDOW + 123)
        across = bytearray(source)
        for boundary in (WINDOW, 2 * WINDOW, 3 * WINDOW):
            across[boundary - 3 : boundary + 3] = corpus.rng.randbytes(6)
        corpus.add(f"{kind}-window-boundaries", kind, seed, len(source), bytes(across), options=["-W", str(WINDOW)])
        shifted = source[:WINDOW - 7] + corpus.rng.randbytes(1000) + source[WINDOW - 7 :]
        corpus.add(f"{kind}-windows-shifted", kind, seed, len(source), shifted, options=["-W", str(WINDOW)])
        corpus.add(f"{kind}-windows-larger", kind, seed, len(source), edited(source, corpus.rng, 6) + corpus.rng.randbytes(WINDOW + 500),
                   options=["-W", str(WINDOW)])


def random_cases(corpus: Corpus, count: int) -> None:
    for kind in KINDS:
        for index in range(count):
            seed = corpus.rng.getrandbits(48)
            size = corpus.rng.choice([0, 1, 7, 255, 4096, 20000, 70000])
            target = edited(source_bytes(seed, size), corpus.rng, corpus.rng.randint(0, 12))
            if corpus.rng.random() < 0.2:
                target += corpus.rng.randbytes(corpus.rng.randint(1, 40000))
            options = ["-W", str(WINDOW)] if kind in XDELTA_KINDS and corpus.rng.random() < 0.5 else []
            corpus.add(f"{kind}-{index}", kind, seed, size, target, options=options)
            # For a small game, and for the bytes next to a change, xdelta3
            # writes data instead of copies of the game, so its check of the
            # target covers no other byte of the game. BPS and UPS check the
            # whole game.
            least = 4096 if kind in XDELTA_KINDS else 2
            if kind in CHECKED and size >= least and corpus.rng.random() < 0.1:
                # One byte changed, given a game with another byte changed.
                changed, flip = corpus.rng.sample(range(size), 2)
                while kind in XDELTA_KINDS and abs(changed - flip) < 256:
                    changed, flip = corpus.rng.sample(range(size), 2)
                corpus.add(f"{kind}-{index}-another-game", kind, seed, size,
                           changed_at(source_bytes(seed, size), changed), options=options, wrong_at=flip)


def run_corpus_test(folder: Path) -> int:
    return subprocess.run(
        ["cargo", "test", "-p", "rominabox-engine", "--test", "patch_corpus", "--", "--nocapture"],
        cwd=ROOT / "desktop",
        env={**os.environ, "ROMINABOX_PATCH_CORPUS": str(folder)},
    ).returncode


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("mode", choices=["fixtures", "crosscheck"])
    parser.add_argument("--cases", type=int, default=200, help="random cases of each format (crosscheck)")
    parser.add_argument("--seed", type=int, default=None, help="the cross-check's random seed (crosscheck)")
    arguments = parser.parse_args()
    if not shutil.which("xdelta3"):
        print("xdelta3 is needed: brew install xdelta", file=sys.stderr)
        return 2
    if arguments.mode == "fixtures":
        FIXTURES.mkdir(parents=True, exist_ok=True)
        for old in FIXTURES.iterdir():
            if old.is_file():
                old.unlink()
        corpus = Corpus(FIXTURES, random.Random(20261003))
        edge_cases(corpus)
        corpus.finish()
        print(f"{len(corpus.cases)} cases in {FIXTURES.relative_to(ROOT)}")
        return 0
    seed = arguments.seed if arguments.seed is not None else random.SystemRandom().getrandbits(32)
    if CROSSCHECK.exists():
        print(f"{CROSSCHECK} already exists; remove it first", file=sys.stderr)
        return 2
    CROSSCHECK.mkdir(parents=True)
    corpus = Corpus(CROSSCHECK, random.Random(seed))
    random_cases(corpus, arguments.cases)
    edge_cases(corpus)
    corpus.finish()
    print(f"seed {seed}: {len(corpus.cases)} cases in {CROSSCHECK.relative_to(ROOT)}", flush=True)
    status = run_corpus_test(CROSSCHECK)
    if status == 0:
        for made in CROSSCHECK.iterdir():
            made.unlink()
        CROSSCHECK.rmdir()
    else:
        print(f"kept {CROSSCHECK} for review", file=sys.stderr)
    return status


if __name__ == "__main__":
    sys.exit(main())
