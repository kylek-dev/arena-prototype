import { engine, error } from "../../state/session";
import { useStore } from "../../state/store";

export function Loading() {
  const state = useStore(engine, (s) => s);
  const failure = useStore(error, (s) => s);
  const characterStatus = state?.character.status;
  const startup = state?.startup;
  // The client keeps rendering behind this overlay until its GPU warmup
  // settles, so play does not begin with shader-compile and upload hitches.
  const ready =
    state &&
    (startup
      ? startup.ready
      : state.frames > 10 && ["ready", "failed", "placeholder"].includes(characterStatus ?? ""));

  if (ready && !failure) {
    return characterStatus === "failed" ? (
      <p id="model-notice" className="model-notice" role="status">
        Character unavailable · procedural fallback active.
      </p>
    ) : null;
  }

  return (
    <section id="loading" className="loading" aria-label="Arena startup" data-keyboard-owner="true">
      <div>
        <span className="eyebrow">ARENA / LOCAL PRACTICE</span>
        <h1>
          A small arena.
          <br />A focused experiment.
        </h1>
        <p id="loading-message" role={failure ? "alert" : "status"}>
          {failure ||
            (!state
              ? "Loading the Rust / WebAssembly client…"
              : startup && startup.phase !== "assets"
                ? "Preparing the arena…"
                : "Loading character and textures…")}
        </p>
        {!failure && <div className="loading-line" />}
        {failure && <button onClick={() => location.reload()}>Reload arena</button>}
        <p>One opponent. No account. No multiplayer.</p>
      </div>
    </section>
  );
}
