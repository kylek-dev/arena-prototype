import { useEffect } from "react";
import { castError } from "../../runtime/cast-feedback";
import { nameplateNotice } from "../../runtime/nameplate-feedback";
import { useStore } from "../../state/store";

export function CombatErrors() {
  const failure = useStore(castError, (s) => s);
  const notice = useStore(nameplateNotice, (s) => s);
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => {
      if (nameplateNotice.get() === notice) nameplateNotice.set(null);
    }, 2500);
    return () => window.clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    if (!failure) return;
    const timer = window.setTimeout(() => {
      if (castError.get() === failure) castError.set(null);
    }, 2500);
    return () => window.clearTimeout(timer);
  }, [failure]);
  return (
    <div className="combat-errors" role="status" aria-label="Combat errors" aria-atomic="true">
      {failure && <div>{failure.message}</div>}
      {notice && <div className="nameplate-notice">{notice.message}</div>}
    </div>
  );
}
