import { useState } from "react";
import { actionList, defaults, label } from "../../domain/bindings/model";
import { useConfigSelector, updateConfig } from "../../state/config";
import { captureMessage, acceptBinding, pendingBinding } from "../../state/binding-capture";
import { interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { Modal } from "../../ui/Dialog";
const actionName = (action: string) =>
  ({
    spellbook: "Spells",
    toggleEnemyNameplates: "Toggle enemy nameplates",
    toggleFriendlyNameplates: "Toggle friendly nameplates",
  })[action] ?? action;
export function BindingConflict() {
  const pending = useStore(pendingBinding, (s) => s);
  return (
    pending && (
      <Modal title="Binding conflict" close={() => pendingBinding.set(null)}>
        <p>
          {label(pending.value)} is assigned to{" "}
          {pending.conflicts
            .map((c) => `${actionName(c.action)} (${c.entry === 0 ? "primary" : "secondary"})`)
            .join(", ")}
          . Replace those bindings?
        </p>
        <div className="row">
          <button onClick={() => acceptBinding(pending.action, pending.entry, pending.value, true)}>
            Replace
          </button>
          <button onClick={() => pendingBinding.set(null)}>Cancel</button>
        </div>
      </Modal>
    )
  );
}
export function BindingsEditor() {
  const bindings = useConfigSelector((c) => c.bindings),
    bars = useConfigSelector((c) => c.bars);
  const capture = useStore(interaction, (s) => s.capture),
    message = useStore(captureMessage, (s) => s);
  const [search, setSearch] = useState(""),
    [category, setCategory] = useState("all");
  const group = (a: string) =>
    a.startsWith("slot:")
      ? "slots"
      : ["forward", "back", "left", "right", "jump"].includes(a)
        ? "movement"
        : [
              "menu",
              "settings",
              "spellbook",
              "quickBind",
              "layout",
              "toggleEnemyNameplates",
              "toggleFriendlyNameplates",
            ].includes(a)
          ? "interface"
          : "selection";
  return (
    <div className="stack">
      <p>
        Physical keyboard codes are independent of printed letters; labels may differ on non-US
        layouts. Unmodified and Shift chords are supported. Ctrl, Alt, Meta, function keys and
        Shift+Tab remain browser/OS shortcuts. Escape cancels capture; clear a binding with its
        Clear button.
      </p>
      <label>
        Search actions
        <input value={search} onChange={(e) => setSearch(e.target.value)} />
      </label>
      <label>
        Category
        <select value={category} onChange={(e) => setCategory(e.target.value)}>
          {["all", "movement", "selection", "interface", "slots"].map((v) => (
            <option key={v}>{v}</option>
          ))}
        </select>
      </label>
      <p role="status">
        {capture ? `Press a key for ${actionName(capture.action)} · Escape cancels` : message}
      </p>
      {capture && (
        <Modal
          title={`Bind ${actionName(capture.action)}`}
          description={`Press a physical key for the ${capture.entry === 0 ? "primary" : "secondary"} binding. Shift chords are supported. ${capture.action === "menu" ? "Escape can be assigned here; use Cancel to leave." : "Escape cancels."} Browser/OS shortcuts remain reserved.`}
          className="capture-dialog"
          escapeBlocked
          close={() => interaction.set({ ...interaction.get(), capture: null })}
        >
          <p role="status">{message || "Waiting for a supported key…"}</p>
          <button onClick={() => interaction.set({ ...interaction.get(), capture: null })}>
            Cancel capture
          </button>
        </Modal>
      )}
      <div className="binding-list">
        {actionList(bars)
          .filter(
            (a) =>
              actionName(a).toLowerCase().includes(search.toLowerCase()) &&
              (category === "all" || group(a) === category),
          )
          .map((action) => (
            <div className="binding-row" key={action}>
              <strong>{actionName(action)}</strong>
              {([0, 1] as const).map((entry) => (
                <div className="row" key={entry}>
                  <button
                    aria-label={`Bind ${actionName(action)} ${entry === 0 ? "primary" : "secondary"}`}
                    className={
                      capture?.action === action && capture.entry === entry ? "selected" : ""
                    }
                    onClick={() => {
                      captureMessage.set("");
                      interaction.set({ ...interaction.get(), capture: { action, entry } });
                    }}
                  >
                    {label(bindings[action]?.[entry])}
                  </button>
                  <button
                    aria-label={`Clear ${actionName(action)} ${entry === 0 ? "primary" : "secondary"}`}
                    onClick={() => acceptBinding(action, entry, null)}
                  >
                    Clear
                  </button>
                </div>
              ))}
            </div>
          ))}
      </div>
      <button onClick={() => updateConfig((c) => ({ ...c, bindings: defaults(c.bars) }))}>
        Restore all default bindings
      </button>
    </div>
  );
}
