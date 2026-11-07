import { useState } from "react";
import { parsePreferences } from "../../domain/preferences/schema";
import {
  importProfiles,
  profileTransition,
  type ProfileCommand,
} from "../../domain/preferences/profiles";
import { preferences, setPreferences, persistence, saveMessage } from "../../state/config";
import { useStore } from "../../state/store";
export function ProfileSettings() {
  const p = useStore(preferences, (s) => s),
    save = useStore(saveMessage, (s) => s);
  const [name, setName] = useState(""),
    [text, setText] = useState(""),
    [message, setMessage] = useState(""),
    [confirm, setConfirm] = useState<"delete" | "restore" | "recover" | null>(null);
  function run(command: ProfileCommand) {
    try {
      setPreferences(profileTransition(preferences.get(), command));
      setMessage("Profile updated.");
      setConfirm(null);
    } catch (e) {
      setMessage(String(e));
    }
  }
  function exportProfiles() {
    const output = JSON.stringify(preferences.get(), null, 2);
    setText(output);
    const url = URL.createObjectURL(new Blob([output], { type: "application/json" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = "arena-local-profiles.json";
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  return (
    <div className="stack">
      <p>
        Profiles are local to this browser. No account synchronization. Import adds profiles with
        unique names; it never overwrites existing profiles.
      </p>
      <label>
        Active profile
        <select value={p.active} onChange={(e) => run({ type: "select", id: e.target.value })}>
          {p.profiles.map((v) => (
            <option key={v.id} value={v.id}>
              {v.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        Profile name
        <input maxLength={40} value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <div className="row">
        <button onClick={() => run({ type: "create", id: crypto.randomUUID(), name })}>
          Create
        </button>
        <button onClick={() => run({ type: "duplicate", id: crypto.randomUUID(), name })}>
          Duplicate active
        </button>
        <button onClick={() => run({ type: "rename", name })}>Rename active</button>
        <button disabled={p.profiles.length === 1} onClick={() => setConfirm("delete")}>
          Delete active
        </button>
        <button onClick={() => setConfirm("restore")}>Restore active defaults</button>
      </div>
      {save && <p role="alert">{save}</p>}
      {save && !persistence.blocked && (
        <button
          onClick={() => {
            persistence.schedule(preferences.get());
            persistence.flush();
          }}
        >
          Retry saving
        </button>
      )}
      {persistence.blocked && (
        <button onClick={() => setConfirm("recover")}>
          Replace preserved storage with current profiles
        </button>
      )}
      {confirm && (
        <div className="notice">
          <p>
            {confirm === "recover"
              ? "Overwrite preserved storage? Export it manually first if you need to keep it."
              : confirm === "delete"
                ? "Delete the active profile and its configuration?"
                : "Restore all defaults for the active profile?"}
          </p>
          <button
            onClick={() => {
              if (confirm === "recover") {
                persistence.schedule(preferences.get());
                persistence.enableWrites();
                setConfirm(null);
              } else
                run(confirm === "delete" ? { type: "delete", id: p.active } : { type: "restore" });
            }}
          >
            Confirm {confirm}
          </button>
          <button onClick={() => setConfirm(null)}>Cancel</button>
        </div>
      )}
      <label>
        Profile JSON (import / export)
        <textarea rows={6} value={text} onChange={(e) => setText(e.target.value)} />
      </label>
      <label>
        Read JSON file
        <input
          type="file"
          accept="application/json,.json"
          onChange={async (e) => {
            const f = e.target.files?.[0];
            if (!f) return;
            if (f.size > 200_000) {
              setMessage("Maximum import size is 200 KB.");
              return;
            }
            try {
              setText(await f.text());
            } catch {
              setMessage("Could not read file.");
            }
          }}
        />
      </label>
      <div className="row">
        <button onClick={exportProfiles}>Export profiles</button>
        <button
          onClick={() => {
            try {
              setPreferences(
                importProfiles(preferences.get(), parsePreferences(text), () =>
                  crypto.randomUUID(),
                ),
              );
              setMessage("Imported profiles with fresh IDs.");
            } catch (e) {
              setMessage(String(e));
            }
          }}
        >
          Import as new profiles
        </button>
      </div>
      <p role="status">{message}</p>
    </div>
  );
}
