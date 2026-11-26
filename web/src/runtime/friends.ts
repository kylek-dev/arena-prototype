// The browser end of friends (social-identity step 4): reads the list from
// the front door and asks it to change. The door pushes `friends_changed`
// over the queue connection; the page then reads the list again.

import { errorCode, statusCode } from "../domain/account";
import {
  friendDoneText,
  parseFriends,
  type FriendAction,
  type FriendList,
  type Relation,
} from "../domain/friends";
import { createStore } from "../state/store";

type Answer = Pick<Response, "ok" | "status" | "json">;
export type FriendsDeps = {
  fetch(url: string, init?: RequestInit): Promise<Answer>;
  /** The clock reads are spaced on (ms). */
  now(): number;
  /** Runs `run` after `ms`; returns a cancel function. */
  later(run: () => void, ms: number): () => void;
};

/** The door allows 10 list reads at once, then one per 3 s per account. */
export const READ_GAP_MS = 3_000;

export type FriendsState = {
  /** `unavailable` until a list arrives (logged out, or no door). */
  status: "idle" | "loading" | "ready" | "unavailable";
  list: FriendList | null;
  /** The handle an action is waiting on, if any. */
  busy: string | null;
  /** The last refusal (`friendErrorText`). */
  error: string | null;
  /** What the last action did. */
  done: string | null;
};
export const NO_FRIENDS: FriendsState = {
  status: "idle",
  list: null,
  busy: null,
  error: null,
  done: null,
};

const json = async (answer: Answer) => {
  try {
    return (await answer.json()) as unknown;
  } catch {
    return null;
  }
};

export function createFriends(deps: FriendsDeps) {
  const state = createStore<FriendsState>(NO_FRIENDS);
  const update = (patch: Partial<FriendsState>) => state.set({ ...state.get(), ...patch });
  /** Reads the list again. Reads are spaced `READ_GAP_MS` apart (the
   * door's steady rate): requests while one is in flight or waiting are
   * folded into the next read, and a 429 is retried after the gap. */
  let inFlight: Promise<void> | null = null;
  let wanted: { promise: Promise<void>; resolve: () => void } | null = null;
  let cancelTimer: (() => void) | null = null;
  let last = Number.NEGATIVE_INFINITY;
  const read = async () => {
    try {
      const answer = await deps.fetch("/api/friends", {
        cache: "no-store",
        credentials: "same-origin",
      });
      const list = answer.ok ? parseFriends(await json(answer)) : null;
      if (list) update({ status: "ready", list });
      else if (answer.status === 429) void refresh();
      else update({ status: "unavailable", list: null });
    } catch {
      if (state.get().status === "ready") update({ error: "offline" });
      else update({ status: "unavailable", list: null });
    }
  };
  const start = () => {
    last = deps.now();
    inFlight = read().finally(() => {
      inFlight = null;
      schedule();
    });
    return inFlight;
  };
  const schedule = () => {
    if (!wanted || inFlight || cancelTimer) return;
    const run = () => {
      cancelTimer = null;
      const done = wanted;
      wanted = null;
      void start().then(() => done?.resolve());
    };
    const wait = last + READ_GAP_MS - deps.now();
    if (wait <= 0) run();
    else cancelTimer = deps.later(run, wait);
  };
  const refresh = (): Promise<void> => {
    if (!wanted) {
      let resolve = () => {};
      const promise = new Promise<void>((done) => {
        resolve = done;
      });
      wanted = { promise, resolve };
    }
    const promise = wanted.promise;
    if (state.get().status !== "ready") update({ status: "loading" });
    schedule();
    return promise;
  };
  const act = async (action: FriendAction, handle: string) => {
    if (state.get().busy) return false;
    update({ busy: handle, error: null, done: null });
    let answer: Answer;
    try {
      answer = await deps.fetch(`/api/friends/${action}`, {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ handle }),
      });
    } catch {
      update({ busy: null, error: "offline" });
      return false;
    }
    const body = await json(answer);
    if (!answer.ok) {
      update({ busy: null, error: errorCode(body) ?? statusCode(answer.status) });
      return false;
    }
    const relation = (body as { relation?: Relation } | null)?.relation;
    update({ busy: null, done: friendDoneText(action, handle, relation) });
    await refresh();
    return true;
  };
  return {
    state,
    refresh,
    act,
    /** Forgets the list (logged out). */
    clear() {
      cancelTimer?.();
      cancelTimer = null;
      wanted?.resolve();
      wanted = null;
      state.set(NO_FRIENDS);
    },
    dismiss: () => update({ error: null, done: null }),
  };
}
export type Friends = ReturnType<typeof createFriends>;

export function browserFriends() {
  return createFriends({
    fetch: (url, init) => fetch(new URL(url, window.location.origin), init),
    now: () => performance.now(),
    later(run, ms) {
      const timer = window.setTimeout(run, ms);
      return () => window.clearTimeout(timer);
    },
  });
}
