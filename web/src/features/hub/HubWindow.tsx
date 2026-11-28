import { inMatch as inMatchOf } from "../../domain/network";
import { Button } from "@radix-ui/themes";
import { useEffect, useMemo } from "react";
import { invitable } from "../../domain/friends";
import { OUT_OF_HUB, hubNoticeText } from "../../domain/hub";
import { IDLE_QUEUE } from "../../domain/matchmaking";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { NO_FRIENDS } from "../../runtime/friends";
import { enterHub, leaveHub } from "../../runtime/hub";
import { accountService, friendsService } from "../../state/account";
import { requestChat } from "../../state/chat";
import { matchQueue, networkLink } from "../../state/network";
import type { LinkState } from "../../domain/network";
import { engine } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { hubWindowVisible } from "../../state/windows";
import { AdminTag } from "../../ui/AdminTag";
import { DockWindow } from "../../ui/DockWindow";
import { Hint } from "../../ui/Tooltip";
import { Row, RowMenu, type Action } from "../friends/Friends";
import "./hub.css";

const NO_HUB = createStore(OUT_OF_HUB);
const NO_QUEUE = createStore(IDLE_QUEUE);
const NO_ACCOUNT = createStore(LOADING_ACCOUNT);
const NO_LIST = createStore(NO_FRIENDS);
const NO_LINK = createStore<LinkState>({ phase: "idle", attempt: 0, ended: null, status: null });

/** Someone else in this hub world, as the world server relays them. */
type HubPlayer = { id: string; handle: string; admin: boolean };

/** H: the other players in this hub world (ADR 0057), by name, each with
 * Whisper and Invite to group (friends who are on), Add friend (others) and
 * Block in a ⋮ menu. */
export function HubWindow() {
  const queue = useStore(matchQueue, (q) => q);
  const hub = useStore(queue?.hub ?? NO_HUB, (h) => h);
  const inviting = useStore(queue?.state ?? NO_QUEUE, (s) => !!s.inviting);
  const link = useStore(networkLink, (l) => l);
  const inMatch = useStore(link?.state ?? NO_LINK, inMatchOf);

  const account = useStore(accountService, (a) => a);
  const me = useStore(account?.state ?? NO_ACCOUNT, (s) => s.me);
  const friends = useStore(friendsService, (f) => f);
  const list = useStore(friends?.state ?? NO_LIST, (s) => s.list);
  const loggedIn = !!me?.account && !!me.handle;
  // Everyone else the world shows: its players, named by the relay.
  const roster = useStore(engine, (s) => (s?.hub ? s.roster : undefined));
  const players: HubPlayer[] = useMemo(
    () =>
      (roster ?? [])
        .filter((unit) => unit.tag.startsWith("Team") && unit.handle)
        .map((unit) => ({ id: unit.id, handle: unit.handle as string, admin: !!unit.admin }))
        .sort((a, b) => a.handle.localeCompare(b.handle)),
    [roster],
  );
  // Friends decide what the menus offer: read the list when the window opens.
  useEffect(() => {
    if (loggedIn && friends) void friends.refresh();
  }, [loggedIn, friends]);
  const actions = (player: HubPlayer): Action[] => {
    const friend = list?.friends.find(
      (entry) =>
        entry.relation === "friend" && entry.handle.toLowerCase() === player.handle.toLowerCase(),
    );
    const blocked = list?.blocked.some(
      (handle) => handle.toLowerCase() === player.handle.toLowerCase(),
    );
    if (!loggedIn || !friends || blocked) return [];
    return [
      ...(friend && queue
        ? [{ label: "Whisper", run: () => requestChat(`/w ${player.handle} `) }]
        : []),
      ...(friend && queue && invitable(friend) && !inviting
        ? [{ label: "Invite to group", run: () => queue.invite(player.handle) }]
        : []),
      ...(!friend && !player.handle.startsWith("Guest#")
        ? [{ label: "Add friend", run: () => void friends.act("request", player.handle) }]
        : []),
      ...(!player.handle.startsWith("Guest#")
        ? [
            {
              label: "Block",
              run: () => void friends.act("block", player.handle),
              danger: true,
              confirm: "Confirm block",
            },
          ]
        : []),
    ];
  };
  const notice = hubNoticeText(hub.notice);
  return (
    <DockWindow
      title="Hub"
      keyboardOwner={false}
      close={() => hubWindowVisible.set(false)}
      className="hub-window"
    >
      {hub.status === "in" ? (
        <div className="hub-panel" role="group" aria-label="Hub">
          <p className="hub-status" role="status">
            {players.length === 0
              ? "Nobody else is here."
              : players.length === 1
                ? "1 other player is here."
                : `${players.length} other players are here.`}
          </p>
          {players.length > 0 && (
            <ul className="friend-list" aria-label="Players here">
              {players.map((player) => {
                const menu = actions(player);
                return (
                  <Row key={player.id} handle={player.handle} status="In the hub">
                    {player.admin && <AdminTag />}
                    {menu.length > 0 && <RowMenu handle={player.handle} actions={menu} />}
                  </Row>
                );
              })}
            </ul>
          )}
          {!loggedIn && players.length > 0 && (
            <p className="hub-note">
              Log in on the launcher page to add, whisper or invite players.
            </p>
          )}
          <div className="hub-actions">
            <Hint text="Leave the hub for practice in the arena. The Play window brings you back.">
              <Button variant="soft" color="gray" onClick={leaveHub}>
                Leave the hub
              </Button>
            </Hint>
          </div>
        </div>
      ) : (
        <div className="hub-panel" role="group" aria-label="Hub">
          <p className="hub-status" role="status">
            {hub.status === "joining"
              ? "Entering the hub."
              : !queue
                ? "The hub is unavailable on this server."
                : inMatch
                  ? "You return to the hub when the match ends."
                  : (notice ?? "You are not in the hub.")}
          </p>
          {hub.status === "out" && queue && !inMatch && (
            <div className="hub-actions">
              <Button onClick={() => enterHub()}>Enter the hub</Button>
            </div>
          )}
        </div>
      )}
    </DockWindow>
  );
}
