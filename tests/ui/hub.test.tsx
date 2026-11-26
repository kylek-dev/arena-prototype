import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { HubWindow } from "../../web/src/features/hub/HubWindow";
import { PlayWindow } from "../../web/src/features/launcher/PlayWindow";
import { createAccount } from "../../web/src/runtime/account";
import { createFriends } from "../../web/src/runtime/friends";
import { returnToHub } from "../../web/src/runtime/hub";
import { accountService, friendsService } from "../../web/src/state/account";
import { beforeMatch } from "../../web/src/state/appearance";
import { matchQueue } from "../../web/src/state/network";
import { playTab } from "../../web/src/state/windows";
import { chatTabs, defaultChannel } from "../../web/src/domain/chat";
import { TooltipProvider } from "../../web/src/ui/Tooltip";
import { fakeQueue } from "./fake-queue";

const LOOK = {
  ancestry: "human",
  body: "male",
  skin_tone: "s1",
  hair_color: "h1",
  facial_hair_color: "h1",
  eyes: "e1",
  build: 0,
  groups: [],
  variants: [],
};
const ROOK = { account: "00112233445566ff", admin: false, login: true, handle: "Rook#0042" };

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
});

/** Joins the hub with two others in view: a friend and a stranger. */
function inHub() {
  act(() => {
    fake.queue.joinHub({
      version: 1,
      body: "male",
      ancestry: "human",
      groups: {},
      skinTone: "s1",
      hairColor: "h1",
      facialHairColor: "h1",
      eyes: "e1",
      variants: {},
      build: 0,
    });
  });
  fake.open();
  fake.say({ type: "hub", instance: 2, you: 1, x: 0, z: 24000 });
  fake.say({
    type: "hub_state",
    tick: 1,
    ack: 0,
    x: 0,
    z: 24000,
    y: 0,
    vy: 0,
    mx: 0,
    mz: 0,
    players: [
      [2, 0, 30000, 0, 0],
      [3, 3000, 24000, 0, 0],
    ],
    enter: [
      { id: 2, handle: "Ash#1000", admin: false, appearance: LOOK },
      { id: 3, handle: "Zed#0001", admin: true, appearance: LOOK },
    ],
  });
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

it("joins with the saved look, passes the renderer's numbered moves on, and leaves", () => {
  inHub();
  const sent = fake.sent();
  expect(sent[1]).toMatchObject({ type: "hub_join", appearance: { body: "male", build: 0 } });
  expect(fake.queue.hub.get()).toMatchObject({ status: "in", instance: 2, you: 1 });
  // One move per hub tick, numbered by the renderer, clamped, sent at once.
  fake.queue.moveHub(1, 1000, 0, 16384);
  fake.queue.moveHub(2, 1500, 0, 16384);
  const moves = () => fake.sent().filter((m) => m.type === "hub_move");
  expect(moves()).toEqual([
    { type: "hub_move", seq: 1, x: 1000, z: 0, facing: 16384 },
    { type: "hub_move", seq: 2, x: 1000, z: 0, facing: 16384 },
  ]);
  // Malformed numbers are not sent.
  fake.queue.moveHub(0, 0, 0, 0);
  fake.queue.moveHub(1.5, 0, 0, 0);
  expect(moves()).toHaveLength(2);
  // Moves never take over the refusal bookkeeping: a chat refusal arriving
  // after a move is still the chat box's.
  fake.queue.say("hub", "hello");
  fake.queue.moveHub(3, 0, 0, 0);
  fake.say({ type: "refused", code: "not_in_hub" });
  expect(fake.queue.chat.get().error).toBe("not_in_hub");
  fake.queue.leaveHub();
  expect(fake.sent().at(-1)).toEqual({ type: "hub_leave" });
  expect(fake.queue.hub.get().status).toBe("out");
  // Outside the hub, moves go nowhere.
  fake.queue.moveHub(4, 1000, 0, 0);
  expect(moves()).toHaveLength(3);
});

it("lists the players nearby with what each can be offered", async () => {
  await logIn();
  inHub();
  await act(async () => {
    render(<HubWindow />, { wrapper: TooltipProvider });
  });
  expect(screen.getByRole("status").textContent).toBe("Hub 2. 2 players are nearby.");
  const rows = within(screen.getByRole("list", { name: "Players nearby" })).getAllByRole(
    "listitem",
  );
  // Nearest first.
  expect(rows.map((row) => row.querySelector(".friend-handle")?.textContent)).toEqual([
    "Zed#0001",
    "Ash#1000",
  ]);
  expect(rows[0].textContent).toContain("3 m away");
  expect(rows[0].textContent).toContain("Admin");
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
  expect(fake.sent().at(-1)).toEqual({ type: "hub_leave" });
  expect(screen.getByRole("status").textContent).toBe("You are not in the hub.");
  fireEvent.click(screen.getByRole("button", { name: "Enter the hub" }));
  expect(fake.sent().at(-1)?.type).toBe("hub_join");
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
  expect(fake.sent().at(-1)?.type).toBe("hub_join");
  // Without a door, the launcher page.
  matchQueue.set(null);
  act(() => returnToHub());
  expect(beforeMatch.get()).toBe(true);
  matchQueue.set(fake.queue);
  beforeMatch.set(false);
  fake.say({ type: "hub", instance: 1, you: 4, x: 0, z: 24000 });
  playTab.set("practice");
  render(<PlayWindow />, { wrapper: TooltipProvider });
  fireEvent.click(screen.getByRole("button", { name: "Practice in the arena" }));
  expect(fake.sent().at(-1)).toEqual({ type: "hub_leave" });
  expect(screen.getByRole("button", { name: "Return to the hub" })).toBeTruthy();
});

it("ends a join that is refused for any reason, and resets when the connection drops", () => {
  const look = {
    version: 1 as const,
    body: "male",
    ancestry: "human",
    groups: {},
    skinTone: "s1",
    hairColor: "h1",
    facialHairColor: "h1",
    eyes: "e1",
    variants: {},
    build: 0,
  };
  // No door yet: asking is quiet, and the connection's end is no news.
  act(() => {
    fake.queue.joinHub(look);
  });
  act(() => fake.latest().onclose?.());
  expect(fake.queue.state.get().connection).toBe("offline");
  expect(fake.queue.hub.get()).toMatchObject({ status: "out", notice: "lost" });
  // A logged-in player without a name is refused `no_name`.
  act(() => {
    fake.queue.joinHub(look);
  });
  fake.open();
  fake.say({ type: "refused", code: "no_name" });
  expect(fake.queue.hub.get()).toMatchObject({ status: "out", notice: "no_name" });
  expect(fake.queue.chat.get().error).toBeNull();
  // In the hub, a lost connection leaves it.
  act(() => {
    fake.queue.joinHub(look);
  });
  fake.say({ type: "hub", instance: 1, you: 2, x: 0, z: 24000 });
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

it("hands the renderer the hub, and only a new revision when it changed", async () => {
  const { readHub, onHubIntent, onHubScreen, hubScreen } = await import(
    "../../web/src/state/hub-render"
  );
  expect(readHub()[1]).toBe("null");
  const outside = readHub()[0];
  inHub();
  const [revision, json] = readHub() as [number, string];
  expect(revision).toBeGreaterThan(outside as number);
  const view = JSON.parse(json);
  expect(view).toMatchObject({ x: 0, z: 24000, tick: 1 });
  expect(view.players.map((p: { id: number }) => p.id)).toEqual([2, 3]);
  // No kits installed here: no spec, so the renderer stands a plain body in.
  expect(view.players[0].spec).toBeNull();
  expect(readHub()[0]).toBe(revision);
  // The renderer's intent goes to the door through the queue.
  onHubIntent(1, 1000, 0, 0);
  expect(fake.sent().at(-1)).toEqual({ type: "hub_move", seq: 1, x: 1000, z: 0, facing: 0 });
  // The renderer replays the moves after the door's last applied one.
  expect(JSON.parse(readHub()[1] as string).ack).toBe(0);
  fake.say({
    type: "hub_state",
    tick: 2,
    ack: 1,
    x: 720,
    z: 24000,
    y: 0,
    vy: 0,
    mx: 0,
    mz: 0,
    players: [],
  });
  expect(JSON.parse(readHub()[1] as string).ack).toBe(1);
  // Names on screen: malformed entries are dropped, an unchanged list is quiet.
  onHubScreen('[[2,0.5,0.25],[3,2,0.5],["x",0,0]]');
  expect(hubScreen.get()).toEqual([{ id: 2, x: 0.5, y: 0.25 }]);
  const same = hubScreen.get();
  onHubScreen("[[2,0.5,0.25]]");
  expect(hubScreen.get()).toBe(same);
  onHubScreen("not json");
  expect(hubScreen.get()).toBe(same);
  onHubScreen("[]");
});

it("names the players in the world, opens their menu on a click, and hides the combat HUD", async () => {
  const { App } = await import("../../web/src/app/App");
  const { hubScreen } = await import("../../web/src/state/hub-render");
  const { engine } = await import("../../web/src/state/session");
  const { snapshot } = await import("./fixtures");
  const { hubWindowVisible } = await import("../../web/src/state/windows");
  await logIn();
  engine.set(snapshot());
  inHub();
  act(() => hubScreen.set([{ id: 2, x: 0.4, y: 0.3 }]));
  render(<App />);
  // No unit frames, nameplates or action bars in the hub.
  expect(screen.queryByRole("meter")).toBeNull();
  expect(screen.queryByLabelText("World health bars")).toBeNull();
  const name = screen.getByRole("button", { name: "Ash#1000. Open their menu" });
  expect(name.style.left).toBe("40%");
  await act(async () => fireEvent.click(name));
  expect(hubWindowVisible.get()).toBe(true);
  expect(screen.getByRole("menu", { name: "Actions for Ash#1000" })).toBeTruthy();
  hubWindowVisible.set(false);
  act(() => hubScreen.set([]));
  engine.set(null);
});
