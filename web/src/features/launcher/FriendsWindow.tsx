import { Text } from "@radix-ui/themes";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { accountService } from "../../state/account";
import { createStore, useStore } from "../../state/store";
import { friendsWindowVisible } from "../../state/windows";
import { MovableWindow } from "../../ui/MovableWindow";
import { Friends } from "../friends/Friends";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);

/** F: the friends list (playtest inbox). Logging in stays on the home
 * screen, outside the game UI. */
export function FriendsWindow() {
  const visible = useStore(friendsWindowVisible, (v) => v);
  const account = useStore(accountService, (a) => a);
  const state = useStore(account?.state ?? NO_ACCOUNT, (s) => s);
  if (!visible) return null;
  const loggedIn = !!state.me?.account;
  return (
    <MovableWindow
      title="Friends"
      close={() => friendsWindowVisible.set(false)}
      className="friends-window"
      anchor={{ x: 0.85, y: 0.2 }}
    >
      {loggedIn ? (
        <Friends />
      ) : (
        <Text size="2" color="gray">
          {state.status === "loading"
            ? "Checking your account…"
            : state.status === "ready" && state.me?.login
              ? "Log in on the home screen (game menu → Home screen) to add friends."
              : "Friends need an account, which this server does not offer."}
        </Text>
      )}
    </MovableWindow>
  );
}
