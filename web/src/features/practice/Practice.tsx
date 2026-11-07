import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import { dispatch } from "../../runtime/commands";
import { availability, isEngineReady } from "../../domain/abilities/catalog";
import { Button } from "@radix-ui/themes";

export function Practice() {
  const state = useStore(engine, (s) => s);
  return (
    <section className="practice" aria-label="Practice round">
      <strong id="round-title">{state?.won ? "Round complete" : "Practice round"}</strong>
      <span>
        {state
          ? `${(state.target.maxHealth - state.target.health) / state.pulse.damage} / ${Math.ceil(state.target.maxHealth / state.pulse.damage)} hits`
          : "Waiting for arena"}
      </span>
      <p id="feedback" role="status">
        {state?.feedback ?? "Loading the arena…"}
      </p>
      <p id="attack-status">{availability(state)}</p>
      <Button
        size="1"
        variant="surface"
        color="gray"
        id="reset"
        disabled={!isEngineReady(state)}
        onClick={() => dispatch("reset")}
      >
        Reset practice
      </Button>
    </section>
  );
}
