import { Button, Text } from "@radix-ui/themes";
import { useEffect, useState } from "react";
import {
  friendErrorText,
  groupFriends,
  invitable,
  normalizeHandle,
  presenceLabel,
  type Friend,
} from "../../domain/friends";
import { IDLE_QUEUE } from "../../domain/matchmaking";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { NO_FRIENDS, type Friends as FriendsService } from "../../runtime/friends";
import type { Queue } from "../../runtime/matchmaking";
import { accountService, friendsService } from "../../state/account";
import { matchQueue } from "../../state/network";
import { createStore, useStore } from "../../state/store";
import "./friends.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);
const NO_LIST = createStore(NO_FRIENDS);
const NO_QUEUE = createStore(IDLE_QUEUE);
/** How often the shown list is read again for presence. */
export const PRESENCE_POLL_MS = 30_000;

/** A button that asks once more before acting (remove, block). */
function Confirming({
  label,
  confirm,
  disabled,
  onConfirm,
}: {
  label: string;
  confirm: string;
  disabled: boolean;
  onConfirm: () => void;
}) {
  const [asking, setAsking] = useState(false);
  if (!asking)
    return (
      <Button
        size="1"
        variant="surface"
        color="gray"
        className="action-button"
        disabled={disabled}
        onClick={() => setAsking(true)}
      >
        {label}
      </Button>
    );
  return (
    <>
      <Button
        size="1"
        color="red"
        className="action-button"
        disabled={disabled}
        onClick={() => {
          setAsking(false);
          onConfirm();
        }}
      >
        {confirm}
      </Button>
      <Button
        size="1"
        variant="surface"
        color="gray"
        className="action-button"
        onClick={() => setAsking(false)}
      >
        Keep
      </Button>
    </>
  );
}

function FriendRow({
  friend,
  friends,
  queue,
  busy,
  inviting,
}: {
  friend: Friend;
  friends: FriendsService;
  queue: Queue | null;
  busy: boolean;
  /** An invite is on its way to the door. */
  inviting: boolean;
}) {
  const presence = friend.presence ?? "offline";
  return (
    <li className="friend">
      <span className={`friend-presence ${presence}`} aria-hidden="true" />
      <span className="friend-name">
        <span className="friend-handle">{friend.handle}</span>
        <Text size="1" color="gray">
          {presenceLabel(presence)}
        </Text>
      </span>
      <span className="friend-actions">
        {queue && invitable(friend) && (
          <Button
            size="1"
            className="action-button"
            aria-label={`Invite ${friend.handle}`}
            disabled={inviting}
            onClick={() => queue.invite(friend.handle)}
          >
            Invite
          </Button>
        )}
        <Confirming
          label="Remove"
          confirm={`Remove ${friend.handle}`}
          disabled={busy}
          onConfirm={() => void friends.act("remove", friend.handle)}
        />
        <Confirming
          label="Block"
          confirm={`Block ${friend.handle}`}
          disabled={busy}
          onConfirm={() => void friends.act("block", friend.handle)}
        />
      </span>
    </li>
  );
}

function AddFriend({ friends, busy }: { friends: FriendsService; busy: boolean }) {
  const [text, setText] = useState("");
  const handle = normalizeHandle(text);
  const add = async () => {
    if (!handle || busy) return;
    if (await friends.act("request", handle)) setText("");
  };
  return (
    <div className="friend-add">
      <input
        value={text}
        maxLength={21}
        placeholder="Name#number"
        aria-label="Add a friend by name and number"
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && void add()}
      />
      <Button
        size="1"
        className="action-button"
        disabled={!handle || busy}
        onClick={() => void add()}
      >
        Add friend
      </Button>
    </div>
  );
}

/** The friends list in Play online (social-identity step 4): presence,
 * requests to answer, adding by `Name#number`, one-click invites, removing,
 * blocking and unblocking. Logged-in players only. */
export function Friends() {
  const account = useStore(accountService, (a) => a);
  const me = useStore(account?.state ?? NO_ACCOUNT, (s) => s.me);
  const friends = useStore(friendsService, (f) => f);
  const state = useStore(friends?.state ?? NO_LIST, (s) => s);
  const queue = useStore(matchQueue, (q) => q);
  const queueState = useStore(queue?.state ?? NO_QUEUE, (s) => s);
  const loggedIn = !!me?.account;
  // Presence changes are not pushed: read the list when the panel shows and
  // every 30 s while it does (well inside the door's read limit).
  useEffect(() => {
    if (!loggedIn || !friends) return;
    void friends.refresh();
    const timer = window.setInterval(() => void friends.refresh(), PRESENCE_POLL_MS);
    return () => window.clearInterval(timer);
  }, [loggedIn, friends]);
  if (!friends || !me?.account) return null;
  if (!me.handle)
    return (
      <div className="friends" role="group" aria-label="Friends">
        <Text size="1" color="gray">
          Choose your name to add friends.
        </Text>
      </div>
    );
  const busy = !!state.busy;
  const groups = state.list ? groupFriends(state.list) : null;
  const online = groups?.friends.filter((f) => f.presence !== "offline").length ?? 0;
  const inviteNotice = queueState.inviteSent && `Invite sent to ${queueState.inviteSent}.`;
  return (
    <div className="friends" role="group" aria-label="Friends">
      <Text size="1" weight="medium">
        Friends{groups ? ` · ${online} online` : ""}
      </Text>
      <AddFriend friends={friends} busy={busy} />
      {(state.error || state.done || inviteNotice) && (
        <div className="friends-notice" role={state.error ? "alert" : "status"}>
          <Text size="1">
            {state.error ? friendErrorText(state.error) : (state.done ?? inviteNotice)}
          </Text>
          <Button
            size="1"
            variant="soft"
            color="gray"
            onClick={() => {
              if (state.error || state.done) friends.dismiss();
              else queue?.dismissInviteSent();
            }}
          >
            OK
          </Button>
        </div>
      )}
      {state.status === "loading" && !groups && (
        <Text size="1" color="gray">
          Loading friends…
        </Text>
      )}
      {state.status === "unavailable" && (
        <Text size="1" color="gray">
          Friends could not be loaded.
        </Text>
      )}
      {groups && (
        <>
          {groups.incoming.length > 0 && (
            <ul className="friend-list" aria-label="Friend requests">
              {groups.incoming.map((friend) => (
                <li key={friend.handle} className="friend">
                  <span className="friend-name">
                    <span className="friend-handle">{friend.handle}</span>
                    <Text size="1" color="gray">
                      Wants to be friends
                    </Text>
                  </span>
                  <span className="friend-actions">
                    <Button
                      size="1"
                      className="action-button"
                      disabled={busy}
                      aria-label={`Accept ${friend.handle}`}
                      onClick={() => void friends.act("accept", friend.handle)}
                    >
                      Accept
                    </Button>
                    <Button
                      size="1"
                      variant="surface"
                      color="gray"
                      className="action-button"
                      disabled={busy}
                      aria-label={`Decline ${friend.handle}`}
                      onClick={() => void friends.act("decline", friend.handle)}
                    >
                      Decline
                    </Button>
                  </span>
                </li>
              ))}
            </ul>
          )}
          {groups.friends.length === 0 && groups.incoming.length === 0 ? (
            <Text size="1" color="gray">
              No friends yet. Add one by their name and number.
            </Text>
          ) : (
            <ul className="friend-list" aria-label="Friends list">
              {groups.friends.map((friend) => (
                <FriendRow
                  key={friend.handle}
                  friend={friend}
                  friends={friends}
                  queue={queue}
                  busy={busy}
                  inviting={!!queueState.inviting}
                />
              ))}
            </ul>
          )}
          {groups.outgoing.length > 0 && (
            <ul className="friend-list" aria-label="Sent requests">
              {groups.outgoing.map((friend) => (
                <li key={friend.handle} className="friend">
                  <span className="friend-name">
                    <span className="friend-handle">{friend.handle}</span>
                    <Text size="1" color="gray">
                      Request sent
                    </Text>
                  </span>
                  <span className="friend-actions">
                    <Button
                      size="1"
                      variant="surface"
                      color="gray"
                      className="action-button"
                      disabled={busy}
                      aria-label={`Withdraw request to ${friend.handle}`}
                      onClick={() => void friends.act("remove", friend.handle)}
                    >
                      Withdraw
                    </Button>
                  </span>
                </li>
              ))}
            </ul>
          )}
          {state.list && state.list.blocked.length > 0 && (
            <details className="friends-blocked">
              <summary>
                <Text size="1">Blocked ({state.list.blocked.length})</Text>
              </summary>
              <ul className="friend-list" aria-label="Blocked players">
                {state.list.blocked.map((handle) => (
                  <li key={handle} className="friend">
                    <span className="friend-handle">{handle}</span>
                    <span className="friend-actions">
                      <Button
                        size="1"
                        variant="surface"
                        color="gray"
                        className="action-button"
                        disabled={busy}
                        aria-label={`Unblock ${handle}`}
                        onClick={() => void friends.act("unblock", handle)}
                      >
                        Unblock
                      </Button>
                    </span>
                  </li>
                ))}
              </ul>
            </details>
          )}
        </>
      )}
    </div>
  );
}
