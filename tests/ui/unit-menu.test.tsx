import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Nameplates } from "../../web/src/features/nameplates/Nameplates";
import { installInput } from "../../web/src/runtime/input-router";
import { afterEach, beforeEach, expect, it } from "vitest";
import { withHandles } from "../../web/src/domain/handles";
import { Frames } from "../../web/src/features/unit-frames/Frames";
import { UnitMenu } from "../../web/src/features/unit-frames/UnitMenu";
import { createAccount } from "../../web/src/runtime/account";
import { createFriends } from "../../web/src/runtime/friends";
import { accountService, friendsService } from "../../web/src/state/account";
import { chatFocusRequest } from "../../web/src/state/chat";
import { matchQueue } from "../../web/src/state/network";
import {
  engine,
  focus,
  interaction,
  nameplateScene,
  selectedLayout,
} from "../../web/src/state/session";
import { closeUnitMenu, unitMenu } from "../../web/src/state/unit-menu";
import { fakeQueue } from "./fake-queue";
import { teamSnapshot } from "./fixtures";

const ME = { account: "00112233445566ff", admin: false, login: true, handle: "Rook#0042" };
const LIST = {
  friends: [
    { handle: "Ash#1000", relation: "friend", presence: "online" },
    { handle: "Pal#0005", relation: "outgoing" },
  ],
  blocked: ["Foe#0002"],
};

/** A 3v3 network match: Ash (a friend) and Zed (a stranger) on my team. */
function match(team: [string, string] = ["Ash#1000", "Zed#0003"], selected: number | null = 1) {
  engine.set(
    withHandles({ ...teamSnapshot(3, selected), networked: true }, [
      "Rook#0042",
      "Foe#0001",
      team[0],
      "Foe#0002",
      team[1],
      "Foe#0003",
    ]),
  );
}

const door = async (url: string) => ({
  ok: true,
  status: 200,
  json: async () => (url === "/api/auth/me" ? ME : served),
});
async function logIn() {
  const fetch = door;
  const account = createAccount({ fetch, navigate: () => {}, pageProtocol: "https:" });
  await account.refresh();
  accountService.set(account);
  const friends = createFriends({ fetch, now: () => 0, later: () => () => {} });
  friendsService.set(friends);
  await friends.refresh();
}

/** What the fake door answers for the friends list. */
let served: object = LIST;
let fake: ReturnType<typeof fakeQueue>;
beforeEach(() => {
  served = LIST;
  fake = fakeQueue();
  matchQueue.set(fake.queue);
  focus.set(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
});
afterEach(() => {
  cleanup();
  closeUnitMenu();
  nameplateScene.set(null);
  matchQueue.set(null);
  accountService.set(null);
  friendsService.set(null);
  engine.set(null);
});

const view = () =>
  render(
    <>
      <Frames />
      <UnitMenu />
    </>,
  );
const items = () =>
  within(screen.getByRole("menu"))
    .getAllByRole("menuitem")
    .map((item) => [item.firstChild?.textContent, item.getAttribute("aria-disabled") === "true"]);

it("offers a friend an invitation, a stranger friendship, and says why not otherwise", async () => {
  await logIn();
  match();
  view();
  fireEvent.contextMenu(screen.getByText("Ash#1000").closest("button")!);
  expect(items()).toEqual([
    ["Whisper", false],
    ["Invite to group", false],
    ["Add friend", true],
    ["Inspect", true],
    ["Challenge to a duel", true],
    ["Focus", false],
    ["Edit layout", false],
  ]);
  // Not yet available: say so.
  expect(screen.getByRole("menuitem", { name: /Inspect/ }).textContent).toContain(
    "not available yet",
  );
  fake.queue.connect();
  fake.open();
  fireEvent.click(screen.getByRole("menuitem", { name: /^Invite to group/ }));
  expect(fake.sent().at(-1)).toEqual({ type: "invite", handle: "Ash#1000" });
  expect(screen.queryByRole("menu")).toBeNull();
  fireEvent.contextMenu(screen.getByText("Zed#0003").closest("button")!);
  expect(items().slice(0, 3)).toEqual([
    ["Whisper", true],
    ["Invite to group", true],
    ["Add friend", false],
  ]);
  expect(screen.getByRole("menuitem", { name: /^Whisper/ }).textContent).toContain(
    "Only friends can be whispered.",
  );
  expect(screen.getByRole("menuitem", { name: /^Invite to group/ }).textContent).toContain(
    "Only friends who are online",
  );
});

it("keeps a practice bot to Focus and Edit layout, and the player frame to its own", () => {
  engine.set(teamSnapshot(3));
  view();
  fireEvent.contextMenu(screen.getByText("Teammate 1").closest("button")!);
  expect(screen.getByRole("menuitem", { name: /^Invite to group/ }).textContent).toContain(
    "Only players can be invited.",
  );
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
  fireEvent.contextMenu(screen.getByRole("region", { name: "player status" }));
  expect(items()).toEqual([
    ["Focus", false],
    ["Edit layout", false],
  ]);
  // A click elsewhere closes it.
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("menu")).toBeNull();
});

it("edits the frame it was opened from, straight in layout mode", () => {
  engine.set(teamSnapshot(3));
  view();
  fireEvent.contextMenu(screen.getByText("Teammate 2").closest("button")!);
  act(() => {
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit layout" }));
  });
  expect(interaction.get().layout).toBe(true);
  expect(selectedLayout.get()).toBe("team");
});

it("still focuses arena (enemy) frames on right-click, with no menu", () => {
  engine.set(teamSnapshot(3));
  view();
  fireEvent.contextMenu(screen.getByText("Enemy Bot 2").closest("button")!);
  expect(screen.queryByRole("menu")).toBeNull();
  expect(focus.get()).toBe(teamSnapshot(3).roster?.[3].id);
});

it("explains guests, pending requests and blocked players", async () => {
  await logIn();
  match(["Guest#0007", "Pal#0005"]);
  view();
  fireEvent.contextMenu(screen.getByText("Guest#0007").closest("button")!);
  expect(screen.getByRole("menu").textContent).toContain("Guests cannot be invited.");
  expect(screen.getByRole("menu").textContent).toContain("Guests cannot be added as friends.");
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  fireEvent.contextMenu(screen.getByText("Pal#0005").closest("button")!);
  expect(screen.getByRole("menu").textContent).toContain("A friend request is already pending.");
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  // A blocked player, here as the target (an enemy in this match).
  act(() => match(["Ash#1000", "Zed#0003"], 3));
  fireEvent.contextMenu(screen.getByRole("region", { name: "target status" }));
  expect(screen.getByRole("menu").textContent).toContain("You have blocked this player.");
});

it("closes when its slot shows someone else, the match ends or layout editing starts", () => {
  engine.set(teamSnapshot(3, 1));
  view();
  fireEvent.contextMenu(screen.getByRole("region", { name: "target status" }));
  expect(screen.getByRole("menu", { name: "Enemy Bot options" })).toBeTruthy();
  // A new target: the menu was for the old one.
  act(() => engine.set(teamSnapshot(3, 3)));
  expect(screen.queryByRole("menu")).toBeNull();
  expect(unitMenu.get()).toBeNull();
  fireEvent.contextMenu(screen.getByRole("region", { name: "player status" }));
  act(() => engine.set(null));
  expect(unitMenu.get()).toBeNull();
  // Nothing stale comes back with the next match.
  act(() => engine.set(teamSnapshot(3)));
  expect(screen.queryByRole("menu")).toBeNull();
  fireEvent.contextMenu(screen.getByRole("region", { name: "player status" }));
  act(() => interaction.set({ ...interaction.get(), layout: true }));
  expect(unitMenu.get()).toBeNull();
});

it("keeps the keyboard under real clicks, and Escape closes only the menu", async () => {
  const user = userEvent.setup();
  engine.set(teamSnapshot(3));
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  const canvas = document.getElementById("arena") as HTMLCanvasElement;
  const router = installInput(canvas, () => {});
  canvas.focus();
  try {
    view();
    await user.pointer({
      keys: "[MouseRight]",
      target: screen.getByText("Teammate 1").closest("button")!,
    });
    // Let the focus policy run (it restores the game's focus in a microtask).
    await act(async () => {});
    const menu = screen.getByRole("menu", { name: "Teammate 1 options" });
    expect(menu.contains(document.activeElement)).toBe(true);
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement?.textContent).toMatch(/^Invite to group/);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(interaction.get().panel).toBeNull();
  } finally {
    router.dispose();
  }
});

it("stays on screen near the edges", () => {
  engine.set(teamSnapshot(3));
  view();
  const original = HTMLElement.prototype.getBoundingClientRect;
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    return this.classList.contains("unit-menu")
      ? ({ width: 240, height: 260, left: 0, top: 0, right: 240, bottom: 260 } as DOMRect)
      : original.call(this);
  };
  try {
    fireEvent.contextMenu(screen.getByRole("region", { name: "player status" }), {
      clientX: innerWidth - 5,
      clientY: innerHeight - 5,
    });
    const menu = screen.getByRole("menu");
    expect(menu.style.left).toBe(`${innerWidth - 240 - 8}px`);
    expect(menu.style.top).toBe(`${innerHeight - 260 - 8}px`);
  } finally {
    HTMLElement.prototype.getBoundingClientRect = original;
  }
});

it("opens from a friendly nameplate (without Edit layout); an enemy's focuses", () => {
  engine.set(teamSnapshot(3));
  nameplateScene.set({
    ...teamSnapshot(3),
    nameplates: [
      { tag: "Team1", x: 0.3, y: 0.4 },
      { tag: "Opponent2", x: 0.5, y: 0.4 },
    ],
  });
  render(
    <>
      <Nameplates />
      <UnitMenu />
    </>,
  );
  fireEvent.contextMenu(screen.getByRole("button", { name: /^Teammate 1 health/ }));
  expect(items().map(([label]) => label)).toEqual([
    "Whisper",
    "Invite to group",
    "Add friend",
    "Inspect",
    "Challenge to a duel",
    "Focus",
  ]);
  fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
  fireEvent.contextMenu(screen.getByRole("button", { name: /^Enemy Bot 2 health/ }));
  expect(screen.queryByRole("menu")).toBeNull();
  expect(focus.get()).toBe(teamSnapshot(3).roster?.[3].id);
});

it("whispers a friend through the chat, and explains a friend in a match", async () => {
  await logIn();
  match();
  view();
  fireEvent.contextMenu(screen.getByText("Ash#1000").closest("button")!);
  fireEvent.click(screen.getByRole("menuitem", { name: /^Whisper/ }));
  expect(chatFocusRequest.get().prefill).toBe("/w Ash#1000 ");
  expect(screen.queryByRole("menu")).toBeNull();
  // The door does not deliver whispers into a live match.
  served = {
    friends: [{ handle: "Ash#1000", relation: "friend", presence: "in_match" }],
    blocked: [],
  };
  await act(async () => {
    // A new list, as read when the menu next opens.
    const friends = createFriends({ fetch: door, now: () => 0, later: () => () => {} });
    friendsService.set(friends);
    await friends.refresh();
  });
  fireEvent.contextMenu(screen.getByText("Ash#1000").closest("button")!);
  expect(screen.getByRole("menuitem", { name: /^Whisper/ }).textContent).toContain(
    "Use team chat.",
  );
});
