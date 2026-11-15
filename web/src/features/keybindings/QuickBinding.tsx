import { interaction } from "../../state/session";
import { useStore } from "../../state/store";
import { captureMessage, acceptBinding } from "../../state/binding-capture";
export function QuickBinding() {
  const mode = useStore(interaction, (s) => s),
    message = useStore(captureMessage, (s) => s);
  if (!mode.quick) return null;
  return (
    <aside className="mode-toolbar" aria-label="Quick binding">
      <strong>Quick binding · gameplay paused</strong>
      <p>
        Hover or Tab to a slot, then press a key (Shift chords supported). Primary binding is
        updated. Use Settings for secondary bindings. Escape exits without casting.
      </p>
      <p role="status">
        {message || (mode.hovered ? `Selected ${mode.hovered}` : "Select a visible slot")}
      </p>
      <button
        disabled={!mode.hovered}
        onClick={() => {
          if (mode.hovered) acceptBinding(`slot:${mode.hovered}`, 0, null);
        }}
      >
        Unbind primary
      </button>
      <button onClick={() => interaction.set({ ...mode, quick: false, hovered: null })}>
        Done
      </button>
    </aside>
  );
}
