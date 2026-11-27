import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invitable } from "../../domain/friends";
import { resolveUnit } from "../../domain/units/tags";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { interactUnit, unitRoster } from "../../runtime/commands";
import { NO_FRIENDS } from "../../runtime/friends";
import { accountService, friendsService } from "../../state/account";
import { matchQueue } from "../../state/network";
import { engine, focus, interaction, selectedLayout, setMode } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { closeUnitMenu, unitMenu } from "../../state/unit-menu";
import { IDLE_QUEUE } from "../../domain/matchmaking";
import "./unit-menu.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);
const NO_LIST = createStore(NO_FRIENDS);
const NO_QUEUE = createStore(IDLE_QUEUE);
/** Space kept between the menu and the screen's edges. */
const EDGE = 8;

type Item = { label: string; run?: () => void; why?: string };

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** The menu a right-click on a unit opens (target, focus, party and player
 * frames, friendly nameplates): invite, add as a friend, inspect and duel
 * (not yet available), focus, and editing the frame's place directly.
 * Arena (enemy) frames keep right-click to focus. */
export function UnitMenu() {
  const request = useStore(unitMenu, (m) => m);
  const state = useStore(engine, (s) => s);
  const focused = useStore(focus, (f) => f);
  const editing = useStore(interaction, (s) => s.layout);
  const queue = useStore(matchQueue, (q) => q);
  const inviting = useStore(queue?.state ?? NO_QUEUE, (s) => !!s.inviting);
  const account = useStore(accountService, (a) => a);
  const me = useStore(account?.state ?? NO_ACCOUNT, (s) => s.me);
  const friends = useStore(friendsService, (f) => f);
  const list = useStore(friends?.state ?? NO_LIST, (s) => s.list);
  const menu = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
  const unit =
    request && state && !state.spectating
      ? resolveUnit(request.tag, unitRoster(state, focused))
      : null;
  // The slot no longer shows the unit the menu was opened on (a new target,
  // a match ended, spectating, layout editing): close, so nothing stale
  // reopens later or acts on someone else.
  const gone = !!request && (editing || !unit || unit.id !== request.unitId);
  useEffect(() => {
    if (gone) closeUnitMenu();
  }, [gone]);
  const loggedIn = !!me?.account && !!me.handle;
  // Friends decide what the menu offers: read the list when it opens.
  useEffect(() => {
    if (request && loggedIn && friends) void friends.refresh();
  }, [request?.unitId, loggedIn, friends]);
  // Measured on screen, then kept inside it.
  useLayoutEffect(() => {
    if (!request || !menu.current) {
      setPlace(null);
      return;
    }
    const { width, height } = menu.current.getBoundingClientRect();
    setPlace({
      left: Math.max(EDGE, Math.min(request.x, innerWidth - width - EDGE)),
      top: Math.max(EDGE, Math.min(request.y, innerHeight - height - EDGE)),
    });
  }, [request, unit?.id]);
  // The keyboard goes into the menu; a click elsewhere closes it.
  useEffect(() => {
    if (!request) return;
    menu.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
    const away = (event: PointerEvent) => {
      if (!menu.current?.contains(event.target as Node)) closeUnitMenu();
    };
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [request]);
  if (!request || !unit || gone) return null;
  const self = request.tag === "Player";
  const handle = unit.handle ?? null;
  const guest = !!handle && handle.startsWith("Guest#");
  const friend = handle
    ? list?.friends.find((entry) => entry.relation === "friend" && same(entry.handle, handle))
    : undefined;
  const pending = handle
    ? list?.friends.some((entry) => entry.relation !== "friend" && same(entry.handle, handle))
    : false;
  const blocked = handle ? list?.blocked.some((entry) => same(entry, handle)) : false;
  const loaded = !!list;
  const invite = (): Pick<Item, "run" | "why"> => {
    if (!handle) return { why: "Only players can be invited." };
    if (!loggedIn) return { why: "Log in on the launcher page to invite players." };
    if (guest) return { why: "Guests cannot be invited." };
    if (!loaded) return { why: "Your friends list is loading." };
    if (blocked) return { why: "You have blocked this player." };
    if (!friend || !invitable(friend))
      return { why: "Only friends who are online can be invited." };
    if (inviting) return { why: "An invitation is already waiting for an answer." };
    if (!queue) return { why: "Invitations are not available at the moment." };
    return { run: () => queue.invite(handle) };
  };
  const befriend = (): Pick<Item, "run" | "why"> => {
    if (!handle) return { why: "Only players can be added as friends." };
    if (!loggedIn) return { why: "Log in on the launcher page to add friends." };
    if (guest) return { why: "Guests cannot be added as friends." };
    if (!loaded || !friends) return { why: "Your friends list is loading." };
    if (blocked) return { why: "You have blocked this player." };
    if (friend) return { why: "This player is already your friend." };
    if (pending) return { why: "A friend request is already pending." };
    return { run: () => void friends.act("request", handle) };
  };
  const items: Item[] = [
    ...(self
      ? []
      : [
          { label: "Invite to group", ...invite() },
          { label: "Add friend", ...befriend() },
          { label: "Inspect", why: "Inspecting players is not available yet." },
          { label: "Challenge to a duel", why: "Duels are not available yet." },
        ]),
    { label: "Focus", run: () => interactUnit(request.tag, "focus") },
    ...(request.frame
      ? [
          {
            label: "Edit layout",
            run: () => {
              // Layout mode with this frame picked, ready to drag or nudge.
              const frame = request.frame as string;
              setMode("layout");
              selectedLayout.set(frame);
              requestAnimationFrame(() =>
                document.querySelector<HTMLElement>(`.placed[aria-label="Edit ${frame}"]`)?.focus(),
              );
            },
          },
        ]
      : []),
  ];
  return (
    <div
      ref={menu}
      className="unit-menu"
      role="menu"
      // Escape closes this menu, not the game menu behind it; and the menu
      // keeps the keyboard while open (the focus policy would otherwise hand
      // it back to the game, whose keys would move the character).
      data-escape-owner="true"
      data-keyboard-owner="true"
      aria-label={`${unit.name} options`}
      style={place ?? { left: request.x, top: request.y, visibility: "hidden" }}
      onContextMenu={(event) => event.preventDefault()}
      onBlur={(event) => {
        if (!menu.current?.contains(event.relatedTarget as Node | null)) closeUnitMenu();
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          closeUnitMenu();
        } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          const all = [
            ...(menu.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]") ?? []),
          ];
          const at = all.indexOf(document.activeElement as HTMLButtonElement);
          const step = event.key === "ArrowDown" ? 1 : -1;
          all[(at + step + all.length) % all.length]?.focus();
        }
      }}
    >
      <div className="unit-menu-title">{unit.name}</div>
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          role="menuitem"
          // Unavailable items stay reachable, so their reason can be read.
          aria-disabled={item.run ? undefined : true}
          className={item.run ? undefined : "unavailable"}
          onClick={() => {
            if (!item.run) return;
            closeUnitMenu();
            item.run();
          }}
        >
          {item.label}
          {item.why && !item.run && <span className="unit-menu-why">{item.why}</span>}
        </button>
      ))}
    </div>
  );
}
