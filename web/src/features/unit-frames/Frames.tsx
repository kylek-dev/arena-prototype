import { useConfigSelector, updateConfig } from "../../state/config";
import { engine, focus, interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { interactUnit, unitRoster } from "../../runtime/commands";
import { resolveUnit, type UnitTag } from "../../domain/units/tags";
import type { Entity } from "../../domain/units/selection";
import { useEffect, useMemo, type ComponentProps } from "react";
import { Placed } from "../../ui/Placed";
import { UnitFrame } from "./UnitFrame";
import { SAMPLE_CAST } from "../cast-bar/PlayerCastBar";
import { hoveredFrame } from "../../runtime/unit-hover";
import { IDLE_QUEUE } from "../../domain/matchmaking";
import { matchQueue } from "../../state/network";
import { createStore } from "../../state/store";
import { openUnitMenu } from "../../state/unit-menu";

const NO_QUEUE = createStore(IDLE_QUEUE);
const NONE: readonly (string | null)[] = [];

function TaggedFrame({
  tag,
  role,
  unit,
  editing,
  selected,
  showCast = true,
  portrait = true,
}: {
  tag: UnitTag;
  role: ComponentProps<typeof UnitFrame>["role"];
  unit: Entity | null;
  editing: boolean;
  selected: boolean;
  showCast?: boolean;
  portrait?: boolean;
}) {
  const hovered = useStore(hoveredFrame, (s) => s === tag);
  useEffect(
    () => () => {
      if (hoveredFrame.get() === tag) hoveredFrame.set(null);
    },
    [tag, unit?.id],
  );
  const frame = <UnitFrame role={role} unit={unit} showCast={showCast} portrait={portrait} />;
  if (editing) return frame;
  return (
    <button
      type="button"
      className="unit-entry"
      data-unit-tag={tag}
      aria-describedby={hovered ? "unit-tooltip" : undefined}
      aria-pressed={selected}
      aria-label={
        tag.startsWith("Opponent")
          ? `Target ${unit?.name ?? tag}`
          : `Target ${tag}: ${unit?.name ?? "—"}`
      }
      onMouseEnter={() => hoveredFrame.set(unit ? tag : null)}
      onMouseLeave={() => {
        if (hoveredFrame.get() === tag) hoveredFrame.set(null);
      }}
      onFocus={() => hoveredFrame.set(unit ? tag : null)}
      onBlur={() => {
        if (hoveredFrame.get() === tag) hoveredFrame.set(null);
      }}
      disabled={!unit}
      onClick={() => interactUnit(tag, "target")}
      onContextMenu={(e) => {
        e.preventDefault();
        // Arena (enemy) frames focus; the others open the unit menu.
        if (role === "opponent") interactUnit(tag, "focus");
        else
          openUnitMenu({
            tag,
            unitId: unit?.id ?? "",
            x: e.clientX,
            y: e.clientY,
            frame: role === "teammate" ? "team" : role,
          });
      }}
    >
      {frame}
    </button>
  );
}

/** A party member who is not in this match: shown with the team, greyed,
 * and not a target. */
function PartyMate({ handle, portrait }: { handle: string | null; portrait: boolean }) {
  return (
    <section
      className={`unit-frame teammate party-away${portrait ? "" : " bare"}`}
      aria-label={`${handle ?? "Party member"} status`}
      // The same menu as the live frames (by handle: they have no unit here).
      onContextMenu={(e) => {
        e.preventDefault();
        if (handle)
          openUnitMenu({ kind: "party", handle, x: e.clientX, y: e.clientY, frame: "team" });
      }}
    >
      <div className="unit-heading">
        <span className="unit-name">
          <strong title={handle ?? undefined}>{handle ?? "Party member"}</strong>
        </span>
        {!portrait && <span className="unit-status">Not in this match</span>}
      </div>
      {portrait && (
        <span className="unit-socket">
          <span className="unit-status">Not in this match</span>
        </span>
      )}
      {/* Empty bars fill the art's troughs. */}
      <div className="health-track" aria-hidden="true" />
      <div className="resource-track" aria-hidden="true" />
    </section>
  );
}

/** Party members missing from the match on screen (all of them in practice). */
function usePartyAway(state: ReturnType<typeof engine.get>) {
  const queue = useStore(matchQueue, (q) => q);
  const mates = useStore(queue?.state ?? NO_QUEUE, (s) => s.party?.mates ?? NONE);
  const roster = state?.networked ? state.roster : undefined;
  return useMemo(() => {
    if (!mates.length) return NONE;
    const here = new Set(
      (roster ?? []).flatMap((unit) => (unit.handle ? [unit.handle.toLowerCase()] : [])),
    );
    return mates.filter((mate) => !mate || !here.has(mate.toLowerCase()));
  }, [mates, roster]);
}

/** A frame's unit tag: the unit's own when it has one (in a hub world the
 * frames are a filtered list, so the position is not the tag), else by
 * position (layout samples). */
function frameTag(unit: object, side: "team" | "opponents", index: number): UnitTag {
  const own = (unit as { tag?: unknown }).tag;
  return typeof own === "string"
    ? (own as UnitTag)
    : (`${side === "team" ? "Team" : "Opponent"}${index + 1}` as UnitTag);
}

export function Frames() {
  const state = useStore(engine, (s) => s),
    focused = useStore(focus, (s) => s),
    editing = useStore(interaction, (s) => s.layout);
  const frames = useConfigSelector((c) => c.frames);
  const castBarAttached = useConfigSelector((c) => c.castBarAttached);
  const selectedId = state ? unitRoster(state, focused).target?.id : null;
  const partyAway = usePartyAway(state);
  // In a hub world (ADR 0057) everyone is friendly: team frames show only
  // the party, and the training enemies take the opponent frames.
  const queue = useStore(matchQueue, (q) => q);
  const mates = useStore(queue?.state ?? NO_QUEUE, (s) => s.party?.mates ?? NONE);
  const inParty = (handle: string | undefined) =>
    !!handle && mates.some((mate) => mate?.toLowerCase() === handle.toLowerCase());
  return (
    <>
      {(["player", "target", "focus"] as const).map((role) => {
        const tag = { player: "Player", target: "Target", focus: "Focus" }[role] as UnitTag;
        const liveUnit = state ? resolveUnit(tag, unitRoster(state, focused)) : null;
        if (!editing && role !== "player" && !liveUnit) return null;
        const maximum = state?.player.maxHealth ?? 100;
        const unit = editing
          ? {
              id: `sample-${role}`,
              name: role === "player" ? "Player" : "Enemy Bot",
              health: Math.round(maximum * 0.75),
              maxHealth: maximum,
              resource: 100,
              maxResource: 100,
              // The attached player cast bar previews inside the player frame.
              cast: role === "player" && castBarAttached ? SAMPLE_CAST : null,
            }
          : liveUnit;
        return (
          <Placed
            key={role}
            id={role}
            value={frames[role]}
            onChange={(l) => updateConfig((c) => ({ ...c, frames: { ...c.frames, [role]: l } }))}
          >
            <TaggedFrame
              tag={tag}
              role={role}
              unit={unit ?? null}
              editing={editing}
              selected={!!unit && unit.id === selectedId}
              showCast={role !== "player" || castBarAttached}
              portrait={frames[role].portrait !== false}
            />
          </Placed>
        );
      })}
      {(["team", "opponents"] as const).map((side) => {
        const units = editing
          ? Array.from({ length: side === "team" ? 2 : 3 }, (_, index) => ({
              id: `sample-${side}-${index}`,
              name: side === "team" ? `Teammate ${index + 1}` : `Enemy Bot ${index + 1}`,
              health: 75,
              maxHealth: 100,
              resource: 100,
              maxResource: 100,
            }))
          : state
            ? unitRoster(state, focused)[side].filter(
                (unit) =>
                  unit.id !== state.player.id &&
                  (!state.hub || side !== "team" || inParty(unit.handle)),
              )
            : [];
        const away = side === "team" && !editing ? partyAway : NONE;
        if (!editing && !units.length && !away.length) return null;
        return (
          <Placed
            key={side}
            id={side}
            value={frames[side]}
            onChange={(l) => updateConfig((c) => ({ ...c, frames: { ...c.frames, [side]: l } }))}
          >
            <div
              className="opponent-list"
              role="group"
              aria-label={side === "team" ? "Team" : "Opponents"}
            >
              {units.map((unit, index) => (
                <TaggedFrame
                  key={unit.id}
                  tag={frameTag(unit, side, index)}
                  role={side === "team" ? "teammate" : "opponent"}
                  unit={unit}
                  editing={editing}
                  selected={unit.id === selectedId}
                  portrait={frames[side].portrait !== false}
                />
              ))}
              {away.map((mate, index) => (
                <PartyMate
                  key={`party-${index}-${mate}`}
                  handle={mate}
                  portrait={frames[side].portrait !== false}
                />
              ))}
            </div>
          </Placed>
        );
      })}
    </>
  );
}
