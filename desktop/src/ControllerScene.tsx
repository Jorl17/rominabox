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

type Placement = Record<
  string,
  { imageWidth: number; imageHeight: number; imageX: number; imageY: number }
>;

/** The player scene. These values come from the design, copied here to draw it. */
const SCENE = { width: 960, height: 380 };

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
  const [placement, setPlacement] = useState<Placement | null>(null);

  useEffect(() => {
    let live = true;
    fetch("/controllers/placement.json")
      .then((response) => (response.ok ? response.json() : null))
      .then((value) => live && setPlacement(value))
      // A missing placement is not a serious error, because we may not have run
      // the staging step. Then we draw the scene without an illustration.
      .catch(() => live && setPlacement(null));
    return () => {
      live = false;
    };
  }, []);

  if (!profile.image) return null;
  const stem = profile.image.replace(/\.png$/, "");
  const spot = placement?.[stem];

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

  const drawn = profile.controls.filter((control) => !control.group);

  // We draw a stick once, as one object beneath the pad, as in the player,
  // because both gutters are full at seven callouts, and four directions for
  // each stick would cover other buttons.
  const groups = [
    ...new Set(
      profile.controls
        .map((control) => control.group)
        .filter((name): name is string => Boolean(name)),
    ),
  ].sort();

  return (
    <svg
      className="controller-scene"
      viewBox={`0 0 ${SCENE.width} ${SCENE.height}`}
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

      {groups.map((name, index) => (
        <StickStrip
          key={name}
          name={name}
          index={index}
          count={groups.length}
          members={profile.controls.filter((control) => control.group === name)}
          bindings={bindings}
        />
      ))}

      {drawn.map((control) => (
        <Marker
          key={control.id}
          control={control}
          label={bindings[control.id]?.label || control.label}
          binding={bindings[control.id]?.key || control.key}
          active={selected === control.id}
          onSelect={onSelect}
        />
      ))}
    </svg>
  );
}

function Marker({
  control,
  label,
  binding,
  active,
  onSelect,
}: {
  control: ControlDefinition;
  label: string;
  binding: string;
  active: boolean;
  onSelect: (id: string) => void;
}) {
  // The inner edge of the callout is its right side in the left gutter and its
  // left side in the right gutter, so the leader lines meet it there.
  const edge =
    control.calloutX < 400 ? control.calloutX + 196 : control.calloutX;
  const midline = control.calloutY + 27;

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
      <line
        x1={edge}
        y1={midline}
        x2={control.x}
        y2={midline}
        className="controller-leader"
      />
      <line
        x1={control.x}
        y1={midline}
        x2={control.x}
        y2={control.y}
        className="controller-leader"
      />
      <circle
        cx={control.x}
        cy={control.y}
        r={21}
        className="controller-ring"
      />
      <rect
        x={control.calloutX}
        y={control.calloutY}
        width={196}
        height={54}
        className="controller-callout"
      />
      <text
        x={control.calloutX + 10}
        y={control.calloutY + 23}
        className="controller-callout-label"
      >
        {label}
      </text>
      <text
        x={control.calloutX + 10}
        y={control.calloutY + 43}
        className="controller-callout-key"
      >
        {binding}
      </text>
    </g>
  );
}

/** Geometry from the design, at which we draw the strip in the player. */
const GROUP = { width: 236, height: 62, gap: 16, bottomMargin: 12 };

function StickStrip({
  name,
  index,
  count,
  members,
  bindings,
}: {
  name: string;
  index: number;
  count: number;
  members: ControlDefinition[];
  bindings: Record<string, { label?: string; key?: string }>;
}) {
  const total = count * GROUP.width + (count - 1) * GROUP.gap;
  const left = (SCENE.width - total) / 2 + index * (GROUP.width + GROUP.gap);
  const top = SCENE.height - GROUP.height - GROUP.bottomMargin;

  // One anchor for the whole group, from the member that has one, which is
  // the click of the stick.
  const anchor = members.find((control) => control.x !== 0 || control.y !== 0);
  const keys = members
    .filter((control) => /_(plus|minus)$/.test(control.id))
    .map((control) => (bindings[control.id]?.key || control.key).toUpperCase())
    .join(" ");

  return (
    <g className="controller-marker">
      {anchor ? (
        <>
          <line
            x1={anchor.x}
            y1={anchor.y}
            x2={anchor.x}
            y2={top}
            className="controller-leader"
          />
          <line
            x1={anchor.x}
            y1={top}
            x2={left + GROUP.width / 2}
            y2={top}
            className="controller-leader"
          />
          <circle
            cx={anchor.x}
            cy={anchor.y}
            r={21}
            className="controller-ring"
          />
        </>
      ) : null}
      <rect
        x={left}
        y={top}
        width={GROUP.width}
        height={GROUP.height}
        className="controller-callout"
      />
      <text x={left + 12} y={top + 26} className="controller-callout-label">
        {name.replace(/_/g, " ").toUpperCase()}
      </text>
      <text x={left + 12} y={top + 48} className="controller-callout-key">
        {keys}
      </text>
    </g>
  );
}
