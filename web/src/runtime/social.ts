// Ties the social pieces together: a logged-in player keeps the door's
// queue connection open (to hear invites, friend changes and chat), and the
// friends list follows the account and the door's `friends_changed` pushes.
//
// The door keeps one connection per guest id, which every tab of a browser
// shares, and a newer one replaces the older. So only one tab at a time
// listens (`leader`); the others connect only when the player acts there.
// A listening connection that drops is reopened with a backoff, but never
// after another tab replaced it.

import type { Account } from "./account";
import type { Friends } from "./friends";
import type { Queue } from "./matchmaking";

export type SocialDeps = {
  /** Calls `lead` once this tab may hold the listening connection; returns
   * a function that gives the role up. */
  leader(lead: () => void): () => void;
  /** Runs `run` after `ms`; returns a cancel function. */
  later(run: () => void, ms: number): () => void;
  /** Calls `run` whenever the tab becomes visible; returns an unsubscribe. */
  onVisible(run: () => void): () => void;
};

/** Waits before reopening a dropped connection, by attempt. */
export const RECONNECT_MS = [2_000, 5_000, 15_000, 30_000, 60_000] as const;

export function followAccount(
  account: Pick<Account, "state">,
  queue: Pick<Queue, "state" | "connect">,
  friends: Pick<Friends, "refresh" | "clear">,
  deps: SocialDeps,
) {
  let who: string | null = null;
  let leading = false;
  /** Another tab took the connection: leave it there. */
  let replaced = false;
  let attempts = 0;
  let cancelRetry: (() => void) | null = null;
  const stopRetry = () => {
    cancelRetry?.();
    cancelRetry = null;
  };
  const listen = () => {
    stopRetry();
    if (who === null || !leading || replaced) return;
    if (queue.state.get().connection === "offline") queue.connect();
  };
  const onAccount = () => {
    const next = account.state.get().me?.account ?? null;
    if (next === who) return;
    who = next;
    attempts = 0;
    if (next === null) {
      stopRetry();
      friends.clear();
      return;
    }
    listen();
    void friends.refresh();
  };
  let seen = queue.state.get().friendsChanged;
  const onQueue = () => {
    const current = queue.state.get();
    if (current.friendsChanged !== seen) {
      seen = current.friendsChanged;
      if (who !== null && current.friendsChanged !== 0) void friends.refresh();
    }
    if (current.connection === "online") {
      attempts = 0;
      replaced = false;
      stopRetry();
    } else if (current.connection === "offline" && current.ended === "replaced") {
      replaced = true;
      stopRetry();
    } else if (
      current.connection === "offline" &&
      current.ended !== null &&
      who !== null &&
      leading &&
      !replaced &&
      !cancelRetry
    ) {
      // A listening connection dropped (or the door restarted): try again.
      const wait = RECONNECT_MS[Math.min(attempts, RECONNECT_MS.length - 1)];
      attempts += 1;
      cancelRetry = deps.later(() => {
        cancelRetry = null;
        listen();
      }, wait);
    }
  };
  const stopAccount = account.state.subscribe(onAccount);
  const stopQueue = queue.state.subscribe(onQueue);
  const stopVisible = deps.onVisible(() => {
    if (queue.state.get().connection === "offline") listen();
  });
  const stopLeading = deps.leader(() => {
    leading = true;
    listen();
  });
  onAccount();
  return () => {
    stopRetry();
    stopAccount();
    stopQueue();
    stopVisible();
    stopLeading();
  };
}

/** The browser's deps: a Web Lock elects the listening tab (every tab
 * listens where locks are unavailable, as before them). */
export function browserSocial(): SocialDeps {
  return {
    leader(lead) {
      const locks = (navigator as Navigator & { locks?: LockManager }).locks;
      if (!locks) {
        lead();
        return () => {};
      }
      const abort = new AbortController();
      let release: (() => void) | null = null;
      locks
        .request("arena-queue-listener", { signal: abort.signal }, () => {
          lead();
          // Held until this page goes or gives it up.
          return new Promise<void>((resolve) => {
            release = resolve;
          });
        })
        .catch(() => {});
      return () => {
        abort.abort();
        release?.();
      };
    },
    later(run, ms) {
      const timer = window.setTimeout(run, ms);
      return () => window.clearTimeout(timer);
    },
    onVisible(run) {
      const shown = () => {
        if (document.visibilityState === "visible") run();
      };
      document.addEventListener("visibilitychange", shown);
      return () => document.removeEventListener("visibilitychange", shown);
    },
  };
}
