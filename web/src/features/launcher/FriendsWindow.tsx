import { LOADING_ACCOUNT } from "../../runtime/account";
import { accountService } from "../../state/account";
import { createStore, useStore } from "../../state/store";
import { friendsWindowVisible } from "../../state/windows";
import { DockWindow } from "../../ui/DockWindow";
import { Friends } from "../friends/Friends";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);

/** F: the friends list, with Invite to group (restored 2026-11-26).
 * Logging in stays on the launcher page. */
export function FriendsWindow() {
  const account = useStore(accountService, (a) => a);
  const state = useStore(account?.state ?? NO_ACCOUNT, (s) => s);
  const loggedIn = !!state.me?.account;
  return (
    <DockWindow
      title="Friends"
      keyboardOwner={false}
      close={() => friendsWindowVisible.set(false)}
      className="friends-window"
    >
      {loggedIn ? (
        <Friends />
      ) : (
        <p className="friends-empty">
          {state.status === "loading"
            ? "Checking your account."
            : state.status === "ready" && state.me?.login
              ? "Log in on the launcher page to use the friends list."
              : "The friends list requires an account, which this server does not offer."}
        </p>
      )}
    </DockWindow>
  );
}
