import { useEffect } from "react";
import { IDLE_QUEUE, isQueueBarVisible } from "../../domain/matchmaking";
import { castError } from "../../runtime/cast-feedback";
import { nameplateNotice } from "../../runtime/nameplate-feedback";
import { matchQueue } from "../../state/network";
import { createStore, useStore } from "../../state/store";

const NO_QUEUE = createStore(IDLE_QUEUE);

export function CombatErrors() {
  const failure = useStore(castError, (s) => s);
  const notice = useStore(nameplateNotice, (s) => s);
  const queue = useStore(matchQueue, (q) => q);
  const queueVisible = useStore(queue?.state ?? NO_QUEUE, isQueueBarVisible);
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
    <div
      className={`combat-errors${queueVisible ? " below-queue" : ""}`}
      role="status"
      aria-label="Combat errors"
      aria-atomic="true"
    >
      {failure && <div>{failure.message}</div>}
      {notice && <div className="nameplate-notice">{notice.message}</div>}
    </div>
  );
}
