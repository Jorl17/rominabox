import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ControllerScene } from "./ControllerScene";
import { keyName } from "./keys";
import registry from "../controls.json";
import megadrivePlacement from "../assets/controllers/controller-megadrive.json";
import ps1Placement from "../assets/controllers/controller-ps1.json";
import n64Placement from "../assets/controllers/controller-n64.json";
import gamecubePlacement from "../assets/controllers/controller-gamecube.json";
import megadriveLayout from "../public/controllers/controller-megadrive-layout.json";
import ps1Layout from "../public/controllers/controller-ps1-layout.json";
import n64Layout from "../public/controllers/controller-n64-layout.json";
import gamecubeLayout from "../public/controllers/controller-gamecube-layout.json";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

/**
 * In the builder we must draw the pad where we draw it in the player.
 *
 * In the builder we draw the geometry from the exporter, which we stage
 * beside the artwork with `scripts/render_controllers.py --stage-frontend`.
 * We check that the screen shows the staged geometry and not a second
 * computation, and that in the staged geometry the markers are on the artwork.
 *
 * This does NOT prove that the artwork looks right, or that the anchors are
 * on the correct buttons.
 */

/** We record placement in the space of the artwork, twice the scene. */
const ARTWORK_SCALE = 2;

type Profile = (typeof registry.profiles)[number];

function profileNamed(id: string): Profile {
  const found = registry.profiles.find((entry) => entry.id === id);
  if (!found) throw new Error(`no profile '${id}' in the generated registry`);
  return found;
}

function draw(profile: Profile) {
  const container = document.createElement("div");
  document.body.append(container);
  act(() =>
    createRoot(container).render(
      <ControllerScene
        profile={profile as never}
        bindings={{}}
        selected={null}
        onSelect={() => {}}
      />,
    ),
  );
  return container;
}

describe("the builder's controller scene", () => {
  beforeEach(() => {
    // Serve the same files as for the app, so a test cannot pass against data
    // the app never receives.
    const staged = {
      ps1: [ps1Placement, ps1Layout],
      n64: [n64Placement, n64Layout],
      gamecube: [gamecubePlacement, gamecubeLayout],
      megadrive: [megadrivePlacement, megadriveLayout],
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => ({
        ok: true,
        json: async () => {
          const [placement, layout] =
            Object.entries(staged).find(([stem]) =>
              url.includes(`controller-${stem}`),
            )?.[1] ?? staged.megadrive;
          return url.endsWith("-layout.json") ? layout : placement;
        },
      })),
    );
  });

  it("places the artwork where the renderer places it", async () => {
    const container = draw(profileNamed("megadrive"));
    await act(async () => {});

    const image = container.querySelector("image");
    expect(image, "the scene draws its illustration").not.toBeNull();

    const spot = megadrivePlacement;
    expect(Number(image!.getAttribute("x"))).toBe(spot.imageX / ARTWORK_SCALE);
    expect(Number(image!.getAttribute("y"))).toBe(spot.imageY / ARTWORK_SCALE);
    expect(Number(image!.getAttribute("width"))).toBe(
      spot.imageWidth / ARTWORK_SCALE,
    );
    expect(Number(image!.getAttribute("height"))).toBe(
      spot.imageHeight / ARTWORK_SCALE,
    );
  });

  it("draws the rings the exporter placed, not rings of its own", async () => {
    const container = draw(profileNamed("megadrive"));
    await act(async () => {});

    const drawn = [...container.querySelectorAll("circle")].map((ring) => ({
      cx: Number(ring.getAttribute("cx")),
      cy: Number(ring.getAttribute("cy")),
      r: Number(ring.getAttribute("r")),
    }));
    const expected = megadriveLayout.controls.map((placed) => ({
      cx: placed.marker.x + placed.marker.width / 2,
      cy: placed.marker.y + placed.marker.height / 2,
      r: placed.marker.width / 2,
    }));
    expect(drawn.length).toBe(expected.length);
    for (const ring of expected) {
      expect(
        drawn.some(
          (seen) =>
            seen.cx === ring.cx && seen.cy === ring.cy && seen.r === ring.r,
        ),
        `no ring at ${ring.cx},${ring.cy} r${ring.r}; the builder is placing its own`,
      ).toBe(true);
    }
  });

  it("puts every button marker on the artwork, not beside it", async () => {
    const container = draw(profileNamed("megadrive"));
    await act(async () => {});

    const image = container.querySelector("image")!;
    const left = Number(image.getAttribute("x"));
    const top = Number(image.getAttribute("y"));
    const right = left + Number(image.getAttribute("width"));
    const bottom = top + Number(image.getAttribute("height"));

    // The pad and the rings must use one frame. A ring outside the rectangle
    // of the artwork cannot be on a button.
    const rings = [...container.querySelectorAll("circle")];
    expect(rings.length).toBeGreaterThan(0);
    for (const ring of rings) {
      const x = Number(ring.getAttribute("cx"));
      const y = Number(ring.getAttribute("cy"));
      expect(
        x >= left && x <= right && y >= top && y <= bottom,
        `a marker at ${x},${y} is outside the artwork ${left},${top}-${right},${bottom}`,
      ).toBe(true);
    }
  });

  it("draws each stick once rather than once per direction", async () => {
    const container = draw(profileNamed("ps1"));
    await act(async () => {});

    // PlayStation declares eight analogue directions in two groups. We draw
    // each group once, not eight callouts at the origin.
    const labels = [...container.querySelectorAll("text")].map(
      (node) => node.textContent ?? "",
    );
    expect(
      labels.filter((text) => text === "Left stick" || text === "Right stick"),
    ).toEqual(["Left stick", "Right stick"]);
    for (const control of profileNamed("ps1").controls.filter((c) =>
      Boolean((c as { group?: string }).group),
    )) {
      expect(
        labels.some((text) => text === control.label),
        `${control.id} must not get a callout of its own`,
      ).toBe(false);
    }
  });

  it("titles each stick as its pad names it", async () => {
    // The title is the label of the group, not its id, so the GameCube
    // C-stick does not appear as R STICK.
    const container = draw(profileNamed("gamecube"));
    await act(async () => {});
    const labels = [
      ...container.querySelectorAll(".controller-callout-label"),
    ].map((node) => node.textContent);
    expect(labels).toContain("Control stick");
    expect(labels).toContain("C-stick");
  });

  it("lists a stick's directions by the direction each declares, not by its id", async () => {
    // The catalog declares the direction of each member, so we do not infer
    // it from a suffix such as _plus or _minus on the id.
    const member = (id: string, key: string, direction: string) => ({
      id,
      label: id,
      key,
      x: 0,
      y: 0,
      calloutX: 0,
      calloutY: 0,
      group: "stick",
      direction,
    });
    const profile = {
      id: "made-up",
      name: "Made-up pad",
      image: "controller-made-up.png",
      controls: [
        member("north", "t", "up"),
        member("east", "b", "right"),
        member("south", "g", "down"),
        member("west", "v", "left"),
        member("click", "q", "press"),
      ],
    };
    const strip = { x: 300, y: 300, width: 236, height: 62 };
    const layout = {
      scene: { x: 0, y: 0, width: 960, height: 380 },
      controls: [],
      groups: [{ name: "stick", strip, marker: null, leader: [] }],
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => ({
        ok: true,
        json: async () => (url.endsWith("-layout.json") ? layout : null),
      })),
    );
    const container = draw(profile as never);
    await act(async () => {});

    const keys = container.querySelector(
      ".controller-marker .controller-callout-key",
    );
    expect(keys?.textContent).toBe(
      ["t", "b", "g", "v"].map((key) => keyName(key)).join(" "),
    );
  });

  // The stored name is the RetroArch one (num2). In the picture we word it
  // as in the controls table.
  it("words each bound key as the controls table does", async () => {
    const container = draw(profileNamed("n64"));
    await act(async () => {});

    const marker = [...container.querySelectorAll(".controller-marker")].find(
      (node) =>
        node.querySelector(".controller-callout-label")?.textContent ===
        "C-Right",
    );
    expect(marker, "the N64 scene has a C-Right callout").toBeTruthy();
    expect(marker!.querySelector(".controller-callout-key")?.textContent).toBe(
      "2",
    );
    expect(marker!.getAttribute("aria-label")).toBe("C-Right, bound to 2");
  });

  it("draws nothing rather than guessing when the geometry is not staged", async () => {
    // With no staged geometry we draw nothing in the scene. We do not invent a
    // layout, so the builder cannot differ from the exporter.
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: false, json: async () => ({}) })),
    );
    const container = draw(profileNamed("megadrive"));
    await act(async () => {});
    expect(container.querySelector("svg.controller-scene")).toBeNull();
  });
});
