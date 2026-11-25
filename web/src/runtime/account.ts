// The browser end of accounts (ADR 0055): asks the front door who is logged
// in, and logs out or deletes the account. Login itself is a plain link to
// the door, which sends the player to WorkOS AuthKit's hosted page and back.

import { errorCode, logoutTarget, parseMe, statusCode, type Me } from "../domain/account";
import { createStore } from "../state/store";

type Answer = Pick<Response, "ok" | "status" | "json">;
export type AccountDeps = {
  fetch(url: string, init?: RequestInit): Promise<Answer>;
  /** Leaves the page for `url` (WorkOS's logout page). */
  navigate(url: string): void;
  /** The game page's protocol: `http:` pages (local development) may be
   * sent to an `http:` logout page; `https:` pages only to `https:`. */
  pageProtocol: string;
};

export type AccountAction = "logout" | "logout_everywhere" | "delete";
export type AccountState = {
  /** `unavailable` when the door does not answer (local practice, no door). */
  status: "loading" | "unavailable" | "ready";
  me: Me | null;
  /** The action waiting for the door, if any. */
  busy: AccountAction | null;
  /** Why the last action did not work (see `accountErrorText`). */
  error: string | null;
};
export const LOADING_ACCOUNT: AccountState = {
  status: "loading",
  me: null,
  busy: null,
  error: null,
};

const json = async (answer: Answer) => {
  try {
    return (await answer.json()) as unknown;
  } catch {
    return null;
  }
};

export function createAccount(deps: AccountDeps) {
  const state = createStore<AccountState>(LOADING_ACCOUNT);
  const update = (patch: Partial<AccountState>) => state.set({ ...state.get(), ...patch });
  /** Asks the door who is logged in. */
  const refresh = async () => {
    try {
      const answer = await deps.fetch("/api/auth/me", {
        cache: "no-store",
        credentials: "same-origin",
      });
      const me = answer.ok ? parseMe(await json(answer)) : null;
      update(me ? { status: "ready", me, error: null } : { status: "unavailable", me: null });
    } catch {
      // Keep showing what was known (say, right after an action worked)
      // rather than hiding the panel; the player can retry.
      if (state.get().status === "ready") update({ error: "offline" });
      else update({ status: "unavailable", me: null });
    }
  };
  const end = async (action: AccountAction) => {
    if (state.get().busy) return;
    update({ busy: action, error: null });
    let answer: Answer;
    try {
      answer = await deps.fetch(`/api/auth/${action}`, {
        method: "POST",
        credentials: "same-origin",
      });
    } catch {
      update({ busy: null, error: "offline" });
      return;
    }
    if (answer.status === 404) {
      // Login was switched off since the page asked.
      update({ busy: null });
      await refresh();
      return;
    }
    if (!answer.ok) {
      update({ busy: null, error: errorCode(await json(answer)) ?? statusCode(answer.status) });
      return;
    }
    // Logging out also ends WorkOS's session, on its own page, which returns
    // to the game; otherwise its hosted page would log straight back in.
    const target =
      answer.status === 204 ? null : logoutTarget(await json(answer), deps.pageProtocol);
    if (target) {
      deps.navigate(target);
      return;
    }
    update({ busy: null });
    await refresh();
  };
  return {
    state,
    refresh,
    /** Logs this browser out. */
    logout: () => end("logout"),
    /** Ends every session of the account, on every device. */
    logoutEverywhere: () => end("logout_everywhere"),
    /** Deletes the account (after the page has asked the player to confirm). */
    deleteAccount: () => end("delete"),
    /** Clears the last error. */
    dismiss: () => update({ error: null }),
    /** The page came back (from the back/forward cache, or a hidden tab):
     * whatever was waiting is over, and another tab may have logged out. */
    resume() {
      update({ busy: null });
      return refresh();
    },
  };
}
export type Account = ReturnType<typeof createAccount>;

export function browserAccount() {
  const account = createAccount({
    fetch: (url, init) => fetch(new URL(url, window.location.origin), init),
    navigate: (url) => window.location.assign(url),
    pageProtocol: window.location.protocol,
  });
  const restored = (event: PageTransitionEvent) => {
    if (event.persisted) void account.resume();
  };
  const shown = () => {
    if (document.visibilityState === "visible") void account.resume();
  };
  window.addEventListener("pageshow", restored);
  document.addEventListener("visibilitychange", shown);
  return {
    account,
    stop() {
      window.removeEventListener("pageshow", restored);
      document.removeEventListener("visibilitychange", shown);
    },
  };
}
