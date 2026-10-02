"""Derive console packages from the current registries.

A migration tool. We read desktop/systems.json and desktop/controls.json and
write one package per console under integrations/consoles/, keeping every
stable id, alias, extension, default and button coordinate exactly. The port
is mechanical on purpose: we could not prove a hand-written port lossless,
and in the parity test we compare the loaded catalog with these registries.

Rules for where each item goes, so that nothing is declared twice:
  - a controller profile belongs to the console whose id matches it, or to the
    console it is a variant of (megadrive6 -> megadrive, gameboy -> gb)
  - a core component belongs to the first console in systems.json that uses it
  - `retropad` is not ported, because the catalog provides it as a built-in

We classify the support intent of each console: a console is `enabled` only
if its declared core artifact is present in the runtime kit, and otherwise
`planned`.

    uv run python scripts/port_console_packages.py
"""

from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SYSTEMS = ROOT / "desktop/systems.json"
CONTROLS = ROOT / "desktop/controls.json"
ARTWORK = ROOT / "desktop/assets/controllers"
sys.path.insert(0, str(ROOT / "scripts"))
from core_source import core_source  # noqa: E402

KIT_CORES = core_source() / "cores"
PACKAGES = ROOT / "integrations/consoles"
TARGET = "macos-arm64"
SCHEMA = 1

# Header title ranges selected by name in metadata.rs. They are only offsets,
# so we make them data, and a case that requires a branch stays a handler.
HEADER_TITLES = {
    "megadrive": (0x150, 0x180 - 0x150),
    "gb": (0x134, 0x143 - 0x134),
    "gbc": (0x134, 0x143 - 0x134),
    "gba": (0xA0, 0xAC - 0xA0),
    "n64": (0x20, 0x34 - 0x20),
    "atari7800": (17, 49 - 17),
}
# Normalisations that are algorithms, which stay in Rust and are selected by name.
HANDLERS = {"nes": ["ines_header"]}
# Profiles that are a variant of another console's pad rather than their own.
PROFILE_OWNER = {"megadrive6": "megadrive", "gameboy": "gb"}


def owner_of_profile(profile_id: str, console_ids: set[str]) -> str | None:
    if profile_id == "retropad":
        return None
    if profile_id in PROFILE_OWNER:
        return PROFILE_OWNER[profile_id]
    return profile_id if profile_id in console_ids else None


def main() -> int:
    systems = json.loads(SYSTEMS.read_text())["systems"]
    controls = json.loads(CONTROLS.read_text())["profiles"]
    console_ids = {s["id"] for s in systems}
    profiles = {p["id"]: p for p in controls}

    if PACKAGES.exists():
        shutil.rmtree(PACKAGES)

    # A component belongs to the first console that uses it.
    component_owner: dict[str, str] = {}
    for system in systems:
        for core in system.get("cores", []):
            component_owner.setdefault(core["component"], system["id"])

    enabled = planned = 0
    for index, system in enumerate(systems):
        cid = system["id"]
        directory = PACKAGES / cid
        directory.mkdir(parents=True, exist_ok=True)

        cores = system.get("cores", [])
        artifact_present = any((KIT_CORES / c["filename"]).exists() for c in cores)
        intent = "enabled" if artifact_present else "planned"
        enabled += intent == "enabled"
        planned += intent == "planned"

        profile_id = system.get("controllerProfile") or "retropad"
        variants = [p["id"] for p in controls if cid in p.get("systems", [])]
        if profile_id not in variants:
            variants.insert(0, profile_id)

        console = {
            "schemaVersion": SCHEMA,
            "id": cid,
            "name": system["name"],
            "aliases": system.get("aliases", []),
            "content": {
                "extensions": system.get("extensions", []),
                "category": system.get("category", "cartridge"),
            },
            "recognition": {},
            "metadata": {},
            "cores": [{"component": c["component"]} for c in cores],
            "controllers": {"default": profile_id, "variants": variants},
            "support": {TARGET: intent},
            "presentationOrder": index,
        }
        if cid in HEADER_TITLES:
            offset, length = HEADER_TITLES[cid]
            console["recognition"]["headerTitle"] = {"offset": offset, "length": length}
        if cid in HANDLERS:
            console["recognition"]["handlers"] = HANDLERS[cid]
        if system.get("firmware"):
            console["firmware"] = system["firmware"]
        if system.get("catalog"):
            console["metadata"]["catalog"] = {"provider": "no-intro", "name": system["catalog"]}

        (directory / "console.json").write_text(json.dumps(console, indent=2) + "\n")

        # Components introduced by this console.
        for core in cores:
            if component_owner[core["component"]] != cid:
                continue
            components = directory / "components"
            components.mkdir(exist_ok=True)
            (components / f"{core['component']}.json").write_text(
                json.dumps(
                    {
                        "schemaVersion": SCHEMA,
                        "id": core["component"],
                        "name": core["component"].replace("_", " "),
                        "artifacts": {TARGET: core["filename"]},
                        "license": {"spdx": core["license"], "file": core["licenseFile"]},
                        "capabilities": [],
                    },
                    indent=2,
                )
                + "\n"
            )

    # Controller profiles, placed with the console they belong to.
    ported = 0
    order = {p["id"]: i for i, p in enumerate(controls)}
    for profile_id, profile in profiles.items():
        owner = owner_of_profile(profile_id, console_ids)
        if owner is None:
            continue
        directory = PACKAGES / owner / "controllers"
        directory.mkdir(parents=True, exist_ok=True)
        illustrated = bool(profile.get("image"))
        declaration = {
            "schemaVersion": SCHEMA,
            "id": profile_id,
            "name": profile["name"],
            "presentation": (
                {"kind": "illustrated", "image": profile["image"]}
                if illustrated
                else {"kind": "generic"}
            ),
        }
        declaration["controls"] = [
            {
                key: value
                for key, value in (
                    ("id", c["id"]),
                    ("label", c["label"]),
                    ("key", c["key"]),
                    ("x", c.get("x") if illustrated else None),
                    ("y", c.get("y") if illustrated else None),
                    ("calloutX", c.get("calloutX") if illustrated else None),
                    ("calloutY", c.get("calloutY") if illustrated else None),
                )
                if value is not None
            }
            for c in profile["controls"]
        ]
        declaration["presentationOrder"] = order[profile_id]
        if profile.get("coreDevice") is not None:
            declaration["coreDevice"] = profile["coreDevice"]
        (directory / f"{profile_id}.json").write_text(json.dumps(declaration, indent=2) + "\n")
        if illustrated:
            shutil.copy(ARTWORK / profile["image"], directory / profile["image"])
        ported += 1

    print(f"{len(systems)} consoles  ({enabled} enabled, {planned} planned)")
    print(f"{ported} controller profiles ported (retropad is a catalog built-in)")
    print(f"-> {PACKAGES.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
