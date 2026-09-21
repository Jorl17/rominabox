import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ControllerScene } from "./ControllerScene";
import registry from "../controls.json";
import megadrivePlacement from "../assets/controllers/controller-megadrive.json";
import ps1Placement from "../assets/controllers/controller-ps1.json";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

/**
 * In the builder we must draw the pad where we draw it in the player.
 *
 * We check the rectangle of the artwork on the scene, as read from
 * `placement.json`, and that the buttons are on it.
 *
 * This does NOT prove that the artwork looks right, or that the anchors are
 * on the correct buttons. It proves that the pad and its markers use one frame.
 */

const SCENE = { width: 960, height: 380 };
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
    // We fetch placement at runtime in the component. Serve the same file as
    // for the renderer, so the test cannot pass against a different one.
    // Serve each pad its placement file, as in the app, so a test cannot pass
    // against data the app never receives.
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => ({
        ok: true,
        json: async () =>
          url.includes("ps1") ? ps1Placement : megadrivePlacement,
      })),
    );
  });

  it("places the artwork where the renderer places it", async () => {
    const profile = profileNamed("megadrive");
    const container = draw(profile);
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

  it("puts every button marker on the artwork, not beside it", async () => {
    const profile = profileNamed("megadrive");
    const container = draw(profile);
    await act(async () => {});

    const image = container.querySelector("image")!;
    const left = Number(image.getAttribute("x"));
    const top = Number(image.getAttribute("y"));
    const right = left + Number(image.getAttribute("width"));
    const bottom = top + Number(image.getAttribute("height"));

    // The pad and the rings must use one frame. A ring outside the rectangle
    // of the artwork cannot be on a button, whatever else is true.
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
    const profile = profileNamed("ps1");
    const container = draw(profile);
    await act(async () => {});

    // PlayStation declares eight analogue directions in two groups. We draw
    // each group once, not eight callouts at the origin.
    const grouped = profile.controls.filter((control) =>
      Boolean((control as { group?: string }).group),
    );
    expect(grouped.length).toBeGreaterThan(0);

    const labels = [...container.querySelectorAll("text")].map(
      (node) => node.textContent ?? "",
    );
    expect(labels.filter((text) => text.includes("STICK")).length).toBe(2);
    for (const control of grouped) {
      expect(
        labels.some((text) => text === control.label),
        `${control.id} must not get a callout of its own`,
      ).toBe(false);
    }
  });

  it("stays silent when the placement has not been staged", async () => {
    // The staging step is a build task, and we may not have run it. A scene
    // with no artwork is correct. A crash or a pad at the origin is not.
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: false, json: async () => ({}) })),
    );
    const container = draw(profileNamed("megadrive"));
    await act(async () => {});
    expect(container.querySelector("image")).toBeNull();
    expect(container.querySelector("svg.controller-scene")).not.toBeNull();
  });
});
