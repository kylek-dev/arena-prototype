import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { App } from "../../web/src/app/App";
import { initial } from "../../web/src/domain/preferences/schema";
import { createAccount } from "../../web/src/runtime/account";
import { installInput } from "../../web/src/runtime/input-router";
import { beforeMatch, characterKits, customizerVisible } from "../../web/src/state/appearance";
import { accountService, friendsService } from "../../web/src/state/account";
import { createFriends } from "../../web/src/runtime/friends";
import { persistence, preferences } from "../../web/src/state/config";
import { matchQueue } from "../../web/src/state/network";
import { engine, error, focus, interaction, selectedLayout } from "../../web/src/state/session";
import { friendsWindowVisible, playTab, playWindowVisible } from "../../web/src/state/windows";
import { humanKit } from "../domain/kit-fixture";
import { fakeQueue } from "./fake-queue";
import { snapshot } from "./fixtures";

let router: ReturnType<typeof installInput>;

async function logIn(account: string | null) {
  const service = createAccount({
    fetch: async () =>
      Response.json({
        account,
        admin: false,
        login: true,
        handle: account ? "Rook#0042" : null,
        rename_at: null,
      }),
    navigate: () => {},
    pageProtocol: "https:",
  });
  await service.refresh();
  accountService.set(service);
}

beforeEach(() => {
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  preferences.set(initial());
  engine.set(snapshot());
  error.set("");
  focus.set(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  characterKits.set([humanKit()]);
  customizerVisible.set(false);
  playWindowVisible.set(false);
  friendsWindowVisible.set(false);
  playTab.set("queue");
  matchQueue.set(fakeQueue().queue);
  const canvas = document.getElementById("arena") as HTMLCanvasElement;
  router = installInput(canvas, () => {});
  canvas.focus();
});
afterEach(() => {
  router.dispose();
  cleanup();
  matchQueue.set(null);
  accountService.set(null);
  friendsService.set(null);
  characterKits.set([]);
  clearTimeout(persistence.timer);
});

const launcher = () => within(screen.getByRole("navigation", { name: "Windows" }));

it("opens each window from a one-letter button with a full tooltip", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.hover(launcher().getByRole("button", { name: "Play" }));
  await waitFor(() =>
    expect(screen.getByRole("tooltip").textContent).toBe(
      "Play: practice, join a public queue, or host a custom match.",
    ),
  );
  await user.click(launcher().getByRole("button", { name: "Play" }));
  const play = screen.getByRole("dialog", { name: "Play" });
  // Tabs along the bottom, opening on Queue.
  const tabs = within(play).getByRole("tablist", { name: "Play" });
  // Radix renders each label twice (a hidden copy keeps the width when bold).
  expect(
    within(tabs)
      .getAllByRole("tab")
      .map((tab) => tab.textContent?.replace(/^(.+)\1$/, "$1")),
  ).toEqual(["Practice", "Queue", "Host"]);
  expect(within(play).getByRole("tab", { name: /^Queue/, selected: true })).toBeTruthy();
  // The active tab keeps Radix's own state, which draws its bold label and underline.
  expect(
    within(play)
      .getByRole("tab", { name: /^Queue/, selected: true })
      .getAttribute("data-state"),
  ).toBe("active");
  expect(within(play).getByRole("button", { name: "2v2" })).toBeTruthy();
  await user.click(within(play).getByRole("tab", { name: /^Host/ }));
  expect(within(play).getByRole("button", { name: "Host custom match" })).toBeTruthy();
  expect(within(play).getByRole("textbox", { name: "Custom lobby code" })).toBeTruthy();
  await user.click(within(play).getByRole("tab", { name: /^Practice/ }));
  expect(within(play).getByRole("combobox", { name: "Practice mode" })).toBeTruthy();
  expect(within(play).getByRole("button", { name: "Reset practice" })).toBeTruthy();
  // No account or party invitations in game.
  expect(within(play).queryByRole("group", { name: "Party" })).toBeNull();
  expect(within(play).queryByRole("group", { name: "Account" })).toBeNull();
  expect(within(play).queryByRole("button", { name: "Invite friends" })).toBeNull();
  expect(launcher().getByRole("button", { name: "Play" }).getAttribute("aria-pressed")).toBe(
    "true",
  );
  await user.click(launcher().getByRole("button", { name: "Play" }));
  expect(screen.queryByRole("dialog", { name: "Play" })).toBeNull();

  await user.click(launcher().getByRole("button", { name: "Friends" }));
  expect(screen.getByRole("dialog", { name: "Friends" }).textContent).toMatch(/account/);
  await user.click(launcher().getByRole("button", { name: "Spells" }));
  expect(screen.getByRole("dialog", { name: "Spells" })).toBeTruthy();
  await user.click(launcher().getByRole("button", { name: "Spells" }));
  expect(interaction.get().panel).toBeNull();
  await user.click(launcher().getByRole("button", { name: "Customize character" }));
  expect(customizerVisible.get()).toBe(true);
});

it("closes the launcher's windows with Escape before opening the menu", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(launcher().getByRole("button", { name: "Play" }));
  await user.click(launcher().getByRole("button", { name: "Friends" }));
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  await user.keyboard("{Escape}");
  expect(playWindowVisible.get()).toBe(false);
  expect(friendsWindowVisible.get()).toBe(false);
  expect(interaction.get().panel).toBeNull();
  await user.keyboard("{Escape}");
  expect(interaction.get().panel).toBe("menu");
});

it("shows friends to a logged-in player, and asks guests to log in on the launcher page", async () => {
  await logIn(null);
  friendsWindowVisible.set(true);
  const { unmount } = render(<App />);
  expect(screen.getByRole("dialog", { name: "Friends" }).textContent).toMatch(
    /Log in on the launcher page to use the friends list\./,
  );
  unmount();
  await logIn("00112233445566ff");
  friendsService.set(
    createFriends({
      fetch: async () => ({
        ok: true,
        status: 200,
        json: async () => ({ friends: [], blocked: [] }),
      }),
      now: () => 0,
      later: () => () => {},
    }),
  );
  render(<App />);
  expect(
    within(screen.getByRole("dialog", { name: "Friends" })).getByRole("group", {
      name: "Friends",
    }),
  ).toBeTruthy();
});

it("disables Spells while spectating, and moves with the layout editor", async () => {
  engine.set({ ...snapshot(), spectating: true });
  render(<App />);
  const spells = launcher().getByRole("button", { name: "Spells" });
  expect(spells.getAttribute("aria-disabled")).toBe("true");
  spells.click();
  expect(interaction.get().panel).toBeNull();
  act(() => {
    engine.set(snapshot());
    interaction.set({ ...interaction.get(), layout: true });
  });
  act(() => screen.getByRole("button", { name: "Edit launcher" }).focus());
  expect(selectedLayout.get()).toBe("launcher");
});

it("goes back to the launcher page from the menu, outside a match", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Launcher page" }));
  expect(screen.getByRole("region", { name: "Launcher" })).toBeTruthy();
  expect(beforeMatch.get()).toBe(true);
  beforeMatch.set(false);
});

it("shows a party already formed, with Leave party, but no invite or join", () => {
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  fake.say({ type: "party", code: "K7P2QX", members: 2, leader: false, queue: null });
  playWindowVisible.set(true);
  render(<App />);
  const party = within(screen.getByRole("dialog", { name: "Play" })).getByRole("group", {
    name: "Party",
  });
  // The party code stays internal.
  expect(party.textContent).not.toContain("K7P2QX");
  expect(party.textContent).toContain("You are in a party of 2.");
  expect(within(party).queryByRole("button", { name: /Copy invite link/ })).toBeNull();
  within(party).getByRole("button", { name: "Leave party" }).click();
  expect(fake.sent().at(-1)).toEqual({ type: "leave_party" });
});

it("closes the windows when a full-screen overlay opens, so Escape reaches it", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(launcher().getByRole("button", { name: "Play" }));
  await user.click(launcher().getByRole("button", { name: "Customize character" }));
  expect(playWindowVisible.get()).toBe(false);
  // With a window reopened behind the customizer, Escape still closes the customizer.
  act(() => friendsWindowVisible.set(true));
  await user.keyboard("{Escape}");
  expect(customizerVisible.get()).toBe(false);
});

it("opens the Play window from /queue", async () => {
  const { runSlash, pageCommandContext } = await import("../../web/src/runtime/chat-commands");
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  render(<App />);
  act(() => {
    runSlash("/queue", pageCommandContext(fake.queue));
  });
  expect(screen.getByRole("dialog", { name: "Play" })).toBeTruthy();
});

it("opens every window in the dock at the left middle, side by side in one order", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(launcher().getByRole("button", { name: "Play" }));
  await user.click(launcher().getByRole("button", { name: "Friends" }));
  await user.click(launcher().getByRole("button", { name: "Spells" }));
  const dock = document.querySelector(".window-dock") as HTMLElement;
  const windows = [...dock.querySelectorAll(".dock-window")];
  // Spells, Friends, Play, whatever order they were opened in.
  expect(windows.map((w) => w.getAttribute("aria-label"))).toEqual(["Spells", "Friends", "Play"]);
  // Escape closes Spells first, then Friends and Play.
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  await user.keyboard("{Escape}");
  expect(interaction.get().panel).toBeNull();
  expect(
    [...document.querySelectorAll(".dock-window")].map((w) => w.getAttribute("aria-label")),
  ).toEqual(["Friends", "Play"]);
  await user.keyboard("{Escape}");
  expect(document.querySelector(".window-dock")).toBeNull();
  expect(interaction.get().panel).toBeNull();
});

it("opens the Play window directly on a tab by command", async () => {
  const { runSlash, pageCommandContext } = await import("../../web/src/runtime/chat-commands");
  const fake = fakeQueue();
  matchQueue.set(fake.queue);
  render(<App />);
  for (const [command, tab] of [
    ["/practice", "Practice"],
    ["/host", "Host"],
    ["/queue", "Queue"],
  ] as const) {
    act(() => {
      runSlash(command, pageCommandContext(fake.queue));
    });
    expect(screen.getByRole("tab", { name: new RegExp(`^${tab}`), selected: true })).toBeTruthy();
  }
});

it("keeps gameplay keys working after a spell is clicked in the Spells window", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(launcher().getByRole("button", { name: "Spells" }));
  const spells = screen.getByRole("dialog", { name: "Spells" });
  expect(spells.hasAttribute("data-keyboard-owner")).toBe(false);
  act(() => screen.getByRole("button", { name: "Pulse spell" }).focus());
  await user.keyboard("p");
  expect(screen.queryByRole("dialog", { name: "Spells" })).toBeNull();
});

it("shows the selected Play tab and leaves the keys with the game after a tab is clicked", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(launcher().getByRole("button", { name: "Play" }));
  const play = screen.getByRole("dialog", { name: "Play" });
  expect(play.hasAttribute("data-keyboard-owner")).toBe(false);
  await user.click(within(play).getByRole("tab", { name: /^Host/ }));
  expect(within(play).getByRole("tab", { name: /^Host/ }).getAttribute("data-state")).toBe(
    "active",
  );
  expect(
    within(play)
      .getByRole("tab", { name: /^Queue/ })
      .getAttribute("data-state"),
  ).toBe("inactive");
  // The game has the keys again: W walks.
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  await user.keyboard("{w>}");
  expect(router.sample()[1]).toBe(-1);
  await user.keyboard("{/w}");
});
