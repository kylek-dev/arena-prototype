import { useEffect, useState } from "react";

/** `clock()`, refreshed every `everyMs` while `ticking`. */
export function useClock(ticking: boolean, clock: () => number, everyMs = 1000) {
  const [now, setNow] = useState(clock);
  useEffect(() => {
    if (!ticking) return;
    setNow(clock());
    const id = window.setInterval(() => setNow(clock()), everyMs);
    return () => window.clearInterval(id);
  }, [ticking, clock, everyMs]);
  return now;
}
