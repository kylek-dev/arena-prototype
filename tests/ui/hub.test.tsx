import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import type { Snapshot } from "../../web/src/bridge/snapshot";
import { HubWindow } from "../../web/src/features/hub/HubWindow";
import { PlayWindow } from "../../web/src/features/launcher/PlayWindow";
import { createAccount } from "../../web/src/runtime/account";
import { createFriends } from "../../web/src/runtime/friends";
import { returnToHub } from "../../web/src/runtime/hub";
import { accountService, friendsService } from "../../web/src/state/account";
import { beforeMatch } from "../../web/src/state/appearance";
import { matchQueue } from "../../web/src/state/network";
import { engine } from "../../web/src/state/session";
import { playTab } from "../../web/src/state/windows";
import { chatTabs, defaultChannel } from "../../web/src/domain/chat";
import { TooltipProvider } from "../../web/src/ui/Tooltip";
import { fakeQueue } from "./fake-queue";

const ROOK = { account: "00112233445566ff", admin: false, login: true, handle: "Rook#0042" };
const TICKET = "ab".repeat(40);

let fake: ReturnType<typeof fakeQueue>;
beforeEach(() => {
  fake = fakeQueue();
  matchQueue.set(fake.queue);
});
afterEach(() => {
  cleanup();
  matchQueue.set(null);
  accountService.set(null);
  friendsService.set(null);
  beforeMatch.set(false);
  engine.set(null);
});

const unit = (actor: number, tag: string, handle?: string, admin = false) => ({
  actor,
  tag,
  id: `world-actor-${actor}`,
  name: handle ?? tag,
  health: 100,
  maxHealth: 100,
  ...(handle ? { handle } : {}),
  ...(admin ? { admin } : {}),
});

/** Placed in a hub world with two others there (a friend and a stranger)
 * and a training enemy, as the engine shows it. */
function inHub() {
  act(() => {
    fake.queue.joinHub();
  });
  fake.open();
  fake.say({
    type: "world",
    instance: "00000000000000ab",
    server: "wss://world.test/play",
    ticket: TICKET,
    expires: 2_000_000_000,
  });
  act(() =>
    engine.set({
      hub: true,
      networked: true,
      roster: [
        unit(0, "Player", "Rook#0042"),
        unit(1, "Opponent1"),
        unit(3, "Team1", "Ash#1000"),
        unit(4, "Team2", "Zed#0001", true),
      ],
    } as unknown as Snapshot),
  );
}

async function logIn() {
  const fetch = async (url: string) => ({
    ok: true,
    status: 200,
    json: async () =>
      url === "/api/auth/me"
        ? ROOK
        : {
            friends: [{ handle: "Ash#1000", relation: "friend", presence: "online" }],
            blocked: [],
          },
  });
  const account = createAccount({ fetch, navigate: () => {}, pageProtocol: "https:" });
  await account.refresh();
  accountService.set(account);
  const friends = createFriends({ fetch, now: () => 0, later: () => () => {} });
  friendsService.set(friends);
  await friends.refresh();
}

const menuOf = (handle: string) => {
  fireEvent.click(screen.getByRole("button", { name: `Actions for ${handle}` }));
  return within(screen.getByRole("menu", { name: `Actions for ${handle}` }));
};

it("asks the door for a place and joins the world it names like a match", () => {
  inHub();
  expect(fake.sent()[1]).toEqual({ type: "world_join" });
  expect(fake.queue.hub.get()).toEqual({
    status: "in",
    instance: "00000000000000ab",
    notice: null,
  });
  expect(fake.tickets.at(-1)).toEqual({
    server: "wss://world.test/play",
    ticket: TICKET,
    expires: 2_000_000_000,
    world: true,
  });
  fake.queue.leaveHub();
  expect(fake.sent().at(-1)).toEqual({ type: "world_leave" });
  expect(fake.queue.hub.get().status).toBe("out");
  // A world ticket that arrives after leaving is not joined.
  const joined = fake.tickets.length;
  fake.say({
    type: "world",
    instance: "00000000000000ab",
    server: "wss://world.test/play",
    ticket: TICKET,
    expires: 2_000_000_000,
  });
  expect(fake.tickets).toHaveLength(joined);
});

it("lists the other players in the world with what each can be offered", async () => {
  await logIn();
  inHub();
  await act(async () => {
    render(<HubWindow />, { wrapper: TooltipProvider });
  });
  expect(screen.getByRole("status").textContent).toBe("2 other players are here.");
  const rows = within(screen.getByRole("list", { name: "Players here" })).getAllByRole("listitem");
  // By name; training enemies are not players.
  // No admin tag is shown beside an admin's name.
  expect(rows.map((row) => row.querySelector(".friend-handle")?.textContent)).toEqual([
    "Ash#1000",
    "Zed#0001",
  ]);
  expect(rows[1].textContent).not.toContain("Admin");
  const friend = menuOf("Ash#1000")
    .getAllByRole("menuitem")
    .map((item) => item.textContent);
  expect(friend).toEqual(["Whisper", "Invite to group", "Block"]);
  fireEvent.click(screen.getByRole("menuitem", { name: "Invite to group" }));
  expect(fake.sent().at(-1)).toEqual({ type: "invite", handle: "Ash#1000" });
  const stranger = menuOf("Zed#0001")
    .getAllByRole("menuitem")
    .map((item) => item.textContent);
  expect(stranger).toEqual(["Add friend", "Block"]);
  fireEvent.click(screen.getByRole("button", { name: "Leave the hub" }));
  expect(fake.sent().at(-1)).toEqual({ type: "world_leave" });
  expect(screen.getByRole("status").textContent).toBe("You are not in the hub.");
  fireEvent.click(screen.getByRole("button", { name: "Enter the hub" }));
  expect(fake.sent().at(-1)?.type).toBe("world_join");
  fake.say({ type: "refused", code: "hub_full" });
  expect(screen.getByRole("status").textContent).toBe("That hub is full. Try again in a moment.");
});

it("offers guests no player actions and says why", () => {
  inHub();
  render(<HubWindow />, { wrapper: TooltipProvider });
  expect(screen.queryByRole("button", { name: /Actions for/ })).toBeNull();
  expect(screen.getByText(/Log in on the launcher page/)).toBeTruthy();
});

it("chats in the hub, returns there after a match, and practises away from it", () => {
  expect(chatTabs(false, [], true)).toEqual(["all", "hub", "public", "team"]);
  expect(defaultChannel({ seated: false, spectating: false, inHub: true })).toBe("hub");
  expect(defaultChannel({ seated: true, spectating: false, inHub: true })).toBe("team");
  // After a match: back into the hub, not the launcher page.
  fake.queue.connect();
  fake.open();
  act(() => returnToHub());
  expect(beforeMatch.get()).toBe(false);
  expect(fake.sent().at(-1)?.type).toBe("world_join");
  // Without a door, the launcher page.
  matchQueue.set(null);
  act(() => returnToHub());
  expect(beforeMatch.get()).toBe(true);
  matchQueue.set(fake.queue);
  beforeMatch.set(false);
  fake.say({
    type: "world",
    instance: "00000000000000ab",
    server: "wss://world.test/play",
    ticket: TICKET,
    expires: 2_000_000_000,
  });
  playTab.set("practice");
  render(<PlayWindow />, { wrapper: TooltipProvider });
  fireEvent.click(screen.getByRole("button", { name: "Practice in the arena" }));
  expect(fake.sent().at(-1)).toEqual({ type: "world_leave" });
  expect(screen.getByRole("button", { name: "Return to the hub" })).toBeTruthy();
});

it("ends a join that is refused for any reason, follows departures, and resets when the connection drops", () => {
  // No door yet: asking is quiet, and the connection's end is no news.
  act(() => {
    fake.queue.joinHub();
  });
  act(() => fake.latest().onclose?.());
  expect(fake.queue.state.get().connection).toBe("offline");
  expect(fake.queue.hub.get()).toMatchObject({ status: "out", notice: "lost" });
  // A logged-in player without a name is refused `no_name`.
  act(() => {
    fake.queue.joinHub();
  });
  fake.open();
  fake.say({ type: "refused", code: "no_name" });
  expect(fake.queue.hub.get()).toMatchObject({ status: "out", notice: "no_name" });
  expect(fake.queue.chat.get().error).toBeNull();
  // The world server not answering.
  act(() => {
    fake.queue.joinHub();
  });
  fake.say({ type: "refused", code: "hub_unavailable" });
  expect(fake.queue.hub.get()).toMatchObject({ status: "out", notice: "hub_unavailable" });
  // The door says the world connection ended: the page closes its link.
  inHub();
  fake.say({ type: "hub_left", reason: "left" });
  expect(fake.queue.hub.get()).toMatchObject({ status: "out", notice: "left" });
  expect(fake.worldsLeft()).toBe(1);
  // A rename: asked again at once, under the new name.
  inHub();
  fake.say({ type: "hub_left", reason: "renamed" });
  expect(fake.queue.hub.get().status).toBe("joining");
  expect(fake.sent().at(-1)).toEqual({ type: "world_join" });
  // A match ticket took them: no rejoin.
  inHub();
  fake.say({ type: "hub_left", reason: "match" });
  expect(fake.queue.hub.get()).toMatchObject({ status: "out", notice: "match" });
  // In the hub, a lost connection leaves it.
  inHub();
  act(() => fake.latest().onclose?.());
  expect(fake.queue.hub.get().status).toBe("out");
});

it("offers no way into the hub during a match", async () => {
  const { networkLink } = await import("../../web/src/state/network");
  const { createStore } = await import("../../web/src/state/store");
  networkLink.set({
    state: createStore({ phase: "open", attempt: 0, ended: null, status: null }),
  } as never);
  render(<HubWindow />, { wrapper: TooltipProvider });
  expect(screen.getByRole("status").textContent).toBe("You return to the hub when the match ends.");
  expect(screen.queryByRole("button", { name: "Enter the hub" })).toBeNull();
  networkLink.set(null);
});

it("keeps the Play window and the game menu open to the hub (not a match)", async () => {
  const { networkLink } = await import("../../web/src/state/network");
  const { createStore } = await import("../../web/src/state/store");
  const { GameMenu } = await import("../../web/src/features/menu/GameMenu");
  inHub();
  const leave = { count: 0 };
  networkLink.set({
    state: createStore({ phase: "open", attempt: 0, ended: null, status: null, world: true }),
    leave: () => {
      leave.count += 1;
    },
    remembered: () => null,
  } as never);
  playTab.set("practice");
  render(<PlayWindow />, { wrapper: TooltipProvider });
  expect(screen.queryByText(/available outside a match/)).toBeNull();
  expect(screen.getByRole("button", { name: "Practice in the arena" })).toBeTruthy();
  cleanup();
  render(<GameMenu />, { wrapper: TooltipProvider });
  expect(screen.getByText("Hub")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Leave match" })).toBeNull();
  expect(screen.getByRole("button", { name: "Launcher page" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Leave the hub" }));
  expect(fake.sent().at(-1)).toEqual({ type: "world_leave" });
  expect(leave.count).toBe(1);
  networkLink.set(null);
});
