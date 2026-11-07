import { useEffect } from "react";
import { castError } from "../../runtime/cast-feedback";
import { useStore } from "../../state/store";

export function CombatErrors() {
  const failure = useStore(castError, (s) => s);
  useEffect(() => {
    if (!failure) return;
    const timer = window.setTimeout(() => {
      if (castError.get() === failure) castError.set(null);
    }, 2500);
    return () => window.clearTimeout(timer);
  }, [failure]);
  return (
    <div className="combat-errors" role="status" aria-label="Combat errors" aria-atomic="true">
      {failure?.message}
    </div>
  );
}
