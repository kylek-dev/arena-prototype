import { useConfigSelector, updateConfig } from "../../state/config";
import { draggingSpell, interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { Placed } from "../../ui/Placed";
import { ActionSlot } from "./ActionSlot";
export function ActionBars() {
  const bars = useConfigSelector((c) => c.bars);
  const showEmpty = useConfigSelector((c) => c.showEmptySlots);
  const dragging = useStore(draggingSpell, (d) => d);
  // Hidden empty slots still show where they are needed: while dragging a
  // spell onto one, placing the bar, or binding keys to its slots.
  const revealed = useStore(interaction, (m) => m.layout || m.quick);
  const hideEmpty = !showEmpty && !dragging && !revealed;
  return (
    <>
      {bars.map((bar) => (
        <Placed
          key={bar.id}
          id={bar.id}
          value={bar.layout}
          onChange={(l) =>
            updateConfig((c) => ({
              ...c,
              bars: c.bars.map((b) => (b.id === bar.id ? { ...b, layout: l } : b)),
            }))
          }
        >
          <section
            className={`action-bar${hideEmpty ? " hide-empty" : ""}`}
            aria-label={`Action bar ${bar.id}`}
            style={{
              gridTemplateColumns: `repeat(${bar.vertical ? bar.rows : Math.ceil(bar.slots.length / bar.rows)}, var(--action-slot-size, 64px))`,
            }}
          >
            {bar.slots.map((slot, i) => (
              <ActionSlot key={slot.id} slot={slot} number={i + 1} />
            ))}
          </section>
        </Placed>
      ))}
    </>
  );
}
