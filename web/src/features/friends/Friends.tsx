import { Button } from "@radix-ui/themes";
import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  friendErrorText,
  groupFriends,
  invitable,
  normalizeHandle,
  presenceLabel,
  type Friend,
} from "../../domain/friends";
import { IDLE_QUEUE, noticeText } from "../../domain/matchmaking";
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
/** Refusals of an invitation sent from this list. */
const INVITE_REFUSALS = new Set(["no_such_friend", "offline", "unavailable", "not_logged_in"]);
/** How often the shown list is read again for presence. */
export const PRESENCE_POLL_MS = 30_000;
/** The example a player types against. */
const HANDLE_EXAMPLE = "Name#0000";

type Action = { label: string; run: () => void; danger?: boolean; confirm?: string };

/** The ⋮ menu of a row: its actions, asking once more before a dangerous one. */
function RowMenu({ handle, actions }: { handle: string; actions: Action[] }) {
  const [open, setOpen] = useState(false);
  const [asking, setAsking] = useState<Action | null>(null);
  /** Where the menu shows: fixed, under the button, so the scrolling window
   * never clips it. */
  const [at, setAt] = useState<{ top: number; right: number } | null>(null);
  const root = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLSpanElement>(null);
  const close = (refocus = false) => {
    setOpen(false);
    setAsking(null);
    if (refocus) trigger.current?.focus();
  };
  // The first item (or the confirmation) takes focus, for keyboards.
  useEffect(() => {
    if (open) list.current?.querySelector<HTMLButtonElement>("button")?.focus();
  }, [open, asking]);
  return (
    <span
      className="friend-menu"
      ref={root}
      // While open, Escape closes the menu only, not the window.
      data-escape-owner={open ? "true" : undefined}
      onBlur={(event) => {
        if (!root.current?.contains(event.relatedTarget as Node | null)) close();
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape" && open) {
          event.stopPropagation();
          close(true);
        }
      }}
    >
      <button
        ref={trigger}
        type="button"
        className="friend-menu-button"
        aria-label={`Actions for ${handle}`}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(event) => {
          if (open) {
            close();
            return;
          }
          const rect = event.currentTarget.getBoundingClientRect();
          setAt({ top: rect.bottom + 4, right: Math.max(8, innerWidth - rect.right) });
          setOpen(true);
        }}
      >
        ⋮
      </button>
      {open && (
        <span
          ref={list}
          className="friend-menu-list"
          role="menu"
          aria-label={`Actions for ${handle}`}
          style={at ? { top: at.top, right: at.right } : undefined}
        >
          {asking ? (
            <>
              <button
                type="button"
                role="menuitem"
                className="danger"
                onClick={() => {
                  asking.run();
                  close();
                }}
              >
                {asking.confirm}
              </button>
              <button type="button" role="menuitem" onClick={() => setAsking(null)}>
                Cancel
              </button>
            </>
          ) : (
            actions.map((action) => (
              <button
                key={action.label}
                type="button"
                role="menuitem"
                className={action.danger ? "danger" : undefined}
                onClick={() => {
                  if (action.confirm) setAsking(action);
                  else {
                    action.run();
                    close();
                  }
                }}
              >
                {action.label}
              </button>
            ))
          )}
        </span>
      )}
    </span>
  );
}

function Row({
  handle,
  status,
  presence,
  children,
}: {
  handle: string;
  status: string;
  presence?: string;
  children: ReactNode;
}) {
  return (
    <li className="friend">
      <span className={`friend-presence ${presence ?? "none"}`} aria-hidden="true" />
      <span className="friend-name">
        <span className="friend-handle">{handle}</span>
        <span className="friend-status">{status}</span>
      </span>
      <span className="friend-actions">{children}</span>
    </li>
  );
}

/** The Add prompt: a player's name and number. */
function AddFriend({ friends, close }: { friends: FriendsService; close: () => void }) {
  const [text, setText] = useState("");
  const [tried, setTried] = useState(false);
  const handle = normalizeHandle(text);
  const add = async () => {
    setTried(true);
    if (!handle) return;
    if (await friends.act("request", handle)) close();
  };
  return (
    <form
      className="friend-add"
      aria-label="Add a friend"
      onSubmit={(event) => {
        event.preventDefault();
        void add();
      }}
    >
      <label className="friend-add-field">
        <span>Player name and number</span>
        <input
          value={text}
          maxLength={21}
          placeholder={HANDLE_EXAMPLE}
          autoComplete="off"
          spellCheck={false}
          autoFocus
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              close();
            }
          }}
        />
      </label>
      {tried && !handle && (
        <p className="friends-notice" role="alert">
          Enter the player's name and number, for example {HANDLE_EXAMPLE}.
        </p>
      )}
      <div className="friend-add-actions">
        <Button type="button" size="1" variant="soft" color="gray" onClick={close}>
          Cancel
        </Button>
        <Button type="submit" size="1">
          Send request
        </Button>
      </div>
    </form>
  );
}

function friendActions(
  friend: Friend,
  friends: FriendsService,
  queue: Queue | null,
  inviting: boolean,
): Action[] {
  const act = (action: Parameters<FriendsService["act"]>[0]) => () =>
    void friends.act(action, friend.handle);
  return [
    ...(queue && invitable(friend) && !inviting
      ? [{ label: "Invite to group", run: () => queue.invite(friend.handle) }]
      : []),
    ...(queue && friend.presence !== "offline"
      ? [
          {
            label: "Whisper",
            run: () => {
              queue.whisperTo(friend.handle);
              queue.showChat("whisper");
            },
          },
        ]
      : []),
    { label: "Remove friend", run: act("remove"), danger: true, confirm: "Confirm removal" },
    { label: "Block", run: act("block"), danger: true, confirm: "Confirm block" },
  ];
}

/** The friends list (home screen and the F window; social-identity step 4):
 * requests to answer, friends with presence, requests sent and blocked
 * players, each with its actions in a ⋮ menu, and Add at the bottom.
 * Logged-in players only. */
export function Friends() {
  const account = useStore(accountService, (a) => a);
  const me = useStore(account?.state ?? NO_ACCOUNT, (s) => s.me);
  const friends = useStore(friendsService, (f) => f);
  const state = useStore(friends?.state ?? NO_LIST, (s) => s);
  const queue = useStore(matchQueue, (q) => q);
  const queueState = useStore(queue?.state ?? NO_QUEUE, (s) => s);
  const [adding, setAdding] = useState(false);
  const loggedIn = !!me?.account;
  // Presence changes are not pushed: read the list when it shows and every
  // 30 s while it does (well inside the door's read limit).
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
        <p className="friends-empty">Choose a player name before adding friends.</p>
      </div>
    );
  const busy = !!state.busy;
  const groups = state.list ? groupFriends(state.list) : null;
  // A refused invitation is answered here, where it was sent from.
  const inviteRefused =
    queueState.notice && INVITE_REFUSALS.has(queueState.notice)
      ? noticeText(queueState.notice)
      : null;
  const notice = state.error
    ? friendErrorText(state.error)
    : (state.done ??
      inviteRefused ??
      (queueState.inviteSent ? `Invitation sent to ${queueState.inviteSent}.` : null));
  const empty =
    !!groups &&
    !groups.incoming.length &&
    !groups.friends.length &&
    !groups.outgoing.length &&
    !state.list?.blocked.length;
  return (
    <div className="friends" role="group" aria-label="Friends">
      {state.status === "loading" && !groups && <p className="friends-empty">Loading friends.</p>}
      {state.status === "unavailable" && (
        <p className="friends-empty">The friends list could not be loaded.</p>
      )}
      {empty && <p className="friends-empty">Your friends list is empty.</p>}
      {groups && !empty && (
        <ul className="friend-list" aria-label="Friends list">
          {groups.incoming.map((friend) => (
            <Row key={friend.handle} handle={friend.handle} status="Friend request">
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
            </Row>
          ))}
          {groups.friends.map((friend) => (
            <Row
              key={friend.handle}
              handle={friend.handle}
              status={presenceLabel(friend.presence ?? "offline")}
              presence={friend.presence ?? "offline"}
            >
              <RowMenu
                handle={friend.handle}
                actions={friendActions(friend, friends, queue, !!queueState.inviting)}
              />
            </Row>
          ))}
          {groups.outgoing.map((friend) => (
            <Row key={friend.handle} handle={friend.handle} status="Request sent">
              <RowMenu
                handle={friend.handle}
                actions={[
                  {
                    label: "Withdraw request",
                    run: () => void friends.act("remove", friend.handle),
                  },
                ]}
              />
            </Row>
          ))}
          {state.list?.blocked.map((handle) => (
            <Row key={handle} handle={handle} status="Blocked">
              <RowMenu
                handle={handle}
                actions={[{ label: "Unblock", run: () => void friends.act("unblock", handle) }]}
              />
            </Row>
          ))}
        </ul>
      )}
      {notice && (
        <div className="friends-notice" role={state.error || inviteRefused ? "alert" : "status"}>
          <span>{notice}</span>
          <button
            type="button"
            className="friends-dismiss"
            aria-label="Dismiss"
            onClick={() => {
              if (state.error || state.done) friends.dismiss();
              else if (inviteRefused) queue?.dismiss();
              else queue?.dismissInviteSent();
            }}
          >
            ×
          </button>
        </div>
      )}
      {adding ? (
        <AddFriend friends={friends} close={() => setAdding(false)} />
      ) : (
        <div className="friends-footer">
          <Button size="2" disabled={busy} onClick={() => setAdding(true)}>
            Add
          </Button>
        </div>
      )}
    </div>
  );
}
