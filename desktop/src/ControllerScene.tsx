import { useEffect, useState } from "react";

/**
 * Structural, not imported: in the editor we infer the types from the
 * generated `controls.json` and export no named types. We describe only what
 * we read in this scene, so the dependency is clear.
 */
export type ControlDefinition = {
  id: string;
  label: string;
  key: string;
  x: number;
  y: number;
  calloutX: number;
  calloutY: number;
  group?: string;
};

export type ControlProfile = {
  id: string;
  name: string;
  image: string;
  controls: ControlDefinition[];
};

/**
 * The controller, drawn as in the player.
 *
 * We render the PNG for the player from this same SVG. We use a raster in
 * the player only because we build RmlUi in the RetroArch fork without SVG
 * support. The anchors and callout positions come from the same generated
 * `controls.json` that we write in the exporter, so a button that moves in a
 * console package moves here too.
 */

type Spot = {
  imageWidth: number;
  imageHeight: number;
  imageX: number;
  imageY: number;
};

/** A rectangle on the scene, as we place it in the exporter. */
type Rect = { x: number; y: number; width: number; height: number };
/** One straight run of a leader. Horizontals have height 0, verticals width 0. */
type Run = Rect;

type Layout = {
  scene: Rect;
  controls: { id: string; marker: Rect; callout: Rect; leader: Run[] }[];
  groups: { name: string; strip: Rect; marker: Rect | null; leader: Run[] }[];
};

/**
 * All positions come from the exporter, staged beside the artwork.
 *
 * In this file we compute no scene size, callout size, marker radius, gutter
 * threshold or leader route, so we draw every leader in the builder where it
 * is in the shipped game.
 */

/** We render the artwork at twice the scene, so placement is in those units. */
const ARTWORK_SCALE = 2;

export function ControllerScene({
  profile,
  bindings,
  selected,
  onSelect,
}: {
  profile: ControlProfile;
  bindings: Record<string, { label?: string; key?: string }>;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const stem = profile.image.replace(/\.png$/, "");
  const [placement, setPlacement] = useState<Spot | null>(null);
  const [layout, setLayout] = useState<Layout | null>(null);

  useEffect(() => {
    if (!stem) return;
    let live = true;
    function load<T>(name: string, take: (value: T | null) => void) {
      fetch(`/controllers/${name}`)
        .then((response) => (response.ok ? response.json() : null))
        .then((value) => {
          if (live) take(value as T | null);
        })
        // Not a serious error, because we may not have run the staging step.
        // Then we draw nothing in the scene.
        .catch(() => {
          if (live) take(null);
        });
    }
    load<Spot>(`${stem}.json`, setPlacement);
    load<Layout>(`${stem}-layout.json`, setLayout);
    return () => {
      live = false;
    };
  }, [stem]);

  if (!profile.image) return null;
  const spot = placement;

  // We authored the anchors against the artwork placed exactly as in the
  // player, so we apply the same placement here. We record it in the 1920x760
  // space of the artwork, twice the scene, because we render in that
  // space.
  const place = spot
    ? {
        x: spot.imageX / ARTWORK_SCALE,
        y: spot.imageY / ARTWORK_SCALE,
        width: spot.imageWidth / ARTWORK_SCALE,
        height: spot.imageHeight / ARTWORK_SCALE,
      }
    : null;

  // Until we stage the geometry there is nothing to place anything against,
  // so we draw nothing in the scene instead of guessing a layout.
  if (!layout) return null;
  const named = new Map(
    profile.controls.map((control) => [control.id, control]),
  );

  return (
    <svg
      className="controller-scene"
      viewBox={`0 0 ${layout.scene.width} ${layout.scene.height}`}
      role="group"
      aria-label={`${profile.name} layout`}
    >
      {place ? (
        <image
          href={`/controllers/${stem}.svg`}
          x={place.x}
          y={place.y}
          width={place.width}
          height={place.height}
          preserveAspectRatio="none"
        />
      ) : null}

      {layout.groups.map((group) => (
        <StickStrip
          key={group.name}
          placed={group}
          members={profile.controls.filter(
            (control) => control.group === group.name,
          )}
          bindings={bindings}
        />
      ))}

      {layout.controls.map((placed) => {
        const control = named.get(placed.id);
        if (!control) return null;
        return (
          <Marker
            key={control.id}
            control={control}
            placed={placed}
            label={bindings[control.id]?.label || control.label}
            binding={bindings[control.id]?.key || control.key}
            active={selected === control.id}
            onSelect={onSelect}
          />
        );
      })}
    </svg>
  );
}

function Marker({
  control,
  placed,
  label,
  binding,
  active,
  onSelect,
}: {
  control: ControlDefinition;
  placed: { marker: Rect; callout: Rect; leader: Run[] };
  label: string;
  binding: string;
  active: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <g
      className={`controller-marker${active ? " selected" : ""}`}
      onClick={() => onSelect(control.id)}
      role="button"
      tabIndex={0}
      aria-label={`${label}, bound to ${binding}`}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect(control.id);
        }
      }}
    >
      {placed.leader.map((run, index) => (
        <line
          key={index}
          x1={run.x}
          y1={run.y}
          x2={run.x + run.width}
          y2={run.y + run.height}
          className="controller-leader"
        />
      ))}
      <circle
        cx={placed.marker.x + placed.marker.width / 2}
        cy={placed.marker.y + placed.marker.height / 2}
        r={placed.marker.width / 2}
        className="controller-ring"
      />
      <rect
        x={placed.callout.x}
        y={placed.callout.y}
        width={placed.callout.width}
        height={placed.callout.height}
        className="controller-callout"
      />
      <text
        x={placed.callout.x + 10}
        y={placed.callout.y + 23}
        className="controller-callout-label"
      >
        {label}
      </text>
      <text
        x={placed.callout.x + 10}
        y={placed.callout.y + 43}
        className="controller-callout-key"
      >
        {binding}
      </text>
    </g>
  );
}

function StickStrip({
  placed,
  members,
  bindings,
}: {
  placed: { name: string; strip: Rect; marker: Rect | null; leader: Run[] };
  members: ControlDefinition[];
  bindings: Record<string, { label?: string; key?: string }>;
}) {
  const keys = members
    .filter((control) => /_(plus|minus)$/.test(control.id))
    .map((control) => (bindings[control.id]?.key || control.key).toUpperCase())
    .join(" ");

  return (
    <g className="controller-marker">
      {placed.leader.map((run, index) => (
        <line
          key={index}
          x1={run.x}
          y1={run.y}
          x2={run.x + run.width}
          y2={run.y + run.height}
          className="controller-leader"
        />
      ))}
      {placed.marker ? (
        <circle
          cx={placed.marker.x + placed.marker.width / 2}
          cy={placed.marker.y + placed.marker.height / 2}
          r={placed.marker.width / 2}
          className="controller-ring"
        />
      ) : null}
      <rect
        x={placed.strip.x}
        y={placed.strip.y}
        width={placed.strip.width}
        height={placed.strip.height}
        className="controller-callout"
      />
      <text
        x={placed.strip.x + 12}
        y={placed.strip.y + 26}
        className="controller-callout-label"
      >
        {placed.name.replace(/_/g, " ").toUpperCase()}
      </text>
      <text
        x={placed.strip.x + 12}
        y={placed.strip.y + 48}
        className="controller-callout-key"
      >
        {keys}
      </text>
    </g>
  );
}
