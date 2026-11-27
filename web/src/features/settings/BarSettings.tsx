import { useState } from "react";
import { MAX_BARS, MAX_SLOTS, newBar } from "../../domain/action-bars/model";
import { actionList } from "../../domain/bindings/model";
import { CAST_BUFFER_MS } from "../../domain/preferences/schema";
import { useConfigSelector, updateConfig } from "../../state/config";
export function BarSettings() {
  const bars = useConfigSelector((c) => c.bars),
    locked = useConfigSelector((c) => c.assignmentsLocked),
    castBufferMs = useConfigSelector((c) => c.castBufferMs);
  const [message, setMessage] = useState("");
  return (
    <div className="stack">
      <p>
        1–4 bars, 1–12 slots each. Removing bars/slots also removes their bindings. Assignments and
        layout have separate locks.
      </p>
      <label>
        <input
          type="checkbox"
          checked={locked}
          onChange={(e) => updateConfig((c) => ({ ...c, assignmentsLocked: e.target.checked }))}
        />{" "}
        Lock spell assignments
      </label>
      <label>
        Cast buffer: {castBufferMs > 0 ? `${castBufferMs} ms` : "Off"}
        <input
          type="range"
          min={CAST_BUFFER_MS.min}
          max={CAST_BUFFER_MS.max}
          step={CAST_BUFFER_MS.step}
          value={castBufferMs}
          aria-valuetext={castBufferMs > 0 ? `${castBufferMs} milliseconds` : "Off"}
          onChange={(e) => updateConfig((c) => ({ ...c, castBufferMs: Number(e.target.value) }))}
        />
      </label>
      <p>
        Press your next spell up to this long before it is ready (your current cast, the global
        cooldown or its own cooldown), and it casts the moment it is, with no gap from your
        connection's delay.
      </p>
      {bars.map((bar, i) => (
        <fieldset key={bar.id}>
          <legend>
            Bar {i + 1} · {bar.id}
          </legend>
          <label>
            Slots
            <input
              type="number"
              min={1}
              max={MAX_SLOTS}
              value={bar.slots.length}
              onChange={(e) => {
                const n = Math.max(1, Math.min(MAX_SLOTS, Math.round(Number(e.target.value) || 1)));
                updateConfig((c) => {
                  const updated = c.bars.map((b) =>
                    b.id !== bar.id
                      ? b
                      : {
                          ...b,
                          rows: Math.min(b.rows, n),
                          slots: Array.from(
                            { length: n },
                            (_, index) =>
                              b.slots[index] ?? {
                                id: `slot-${crypto.randomUUID()}`,
                                ability: null,
                              },
                          ),
                        },
                  );
                  const list = actionList(updated);
                  return {
                    ...c,
                    bars: updated,
                    bindings: Object.fromEntries(
                      list.map((a) => [a, c.bindings[a] ?? [null, null]]),
                    ),
                  };
                });
                setMessage("Slot count updated; removed slots and their bindings were discarded.");
              }}
            />
          </label>
          <label>
            Rows / vertical columns
            <input
              type="number"
              min={1}
              max={bar.slots.length}
              value={bar.rows}
              onChange={(e) =>
                updateConfig((c) => ({
                  ...c,
                  bars: c.bars.map((b) =>
                    b.id === bar.id
                      ? {
                          ...b,
                          rows: Math.max(
                            1,
                            Math.min(b.slots.length, Math.round(+e.target.value) || 1),
                          ),
                        }
                      : b,
                  ),
                }))
              }
            />
          </label>
          <label>
            <input
              type="checkbox"
              checked={bar.vertical}
              onChange={(e) =>
                updateConfig((c) => ({
                  ...c,
                  bars: c.bars.map((b) =>
                    b.id === bar.id ? { ...b, vertical: e.target.checked } : b,
                  ),
                }))
              }
            />{" "}
            Vertical orientation
          </label>
          <button
            disabled={bars.length <= 1}
            onClick={() => {
              updateConfig((c) => {
                const updated = c.bars.filter((b) => b.id !== bar.id);
                const list = actionList(updated);
                return {
                  ...c,
                  bars: updated,
                  bindings: Object.fromEntries(
                    Object.entries(c.bindings).filter(([a]) => list.includes(a)),
                  ),
                };
              });
              setMessage(`Removed ${bar.id} and its slot bindings.`);
            }}
          >
            Remove bar
          </button>
        </fieldset>
      ))}
      <button
        disabled={bars.length >= MAX_BARS}
        onClick={() =>
          updateConfig((c) => {
            const bar = newBar(`bar-${crypto.randomUUID()}`);
            return {
              ...c,
              bars: [...c.bars, bar],
              bindings: {
                ...c.bindings,
                ...Object.fromEntries(
                  bar.slots.map((s) => [`slot:${s.id}`, [null, null] as [null, null]]),
                ),
              },
            };
          })
        }
      >
        Add action bar
      </button>
      <p role="status">{message}</p>
    </div>
  );
}
