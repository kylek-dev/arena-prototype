import { useConfigSelector, updateConfig } from "../../state/config";
import { Placed } from "../../ui/Placed";
import { ActionSlot } from "./ActionSlot";
export function ActionBars() {
  const bars = useConfigSelector((c) => c.bars);
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
            className="action-bar"
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
