import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { App } from "../../web/src/app/App";
import { initial } from "../../web/src/domain/preferences/schema";
import { installInput } from "../../web/src/runtime/input-router";
import { persistence, preferences, updateConfig } from "../../web/src/state/config";
import { matchQueue } from "../../web/src/state/network";
import { engine, error, focus, interaction, selectedLayout } from "../../web/src/state/session";
import { fakeQueue } from "./fake-queue";
import { snapshot } from "./fixtures";

let router: ReturnType<typeof installInput>;
let fake: ReturnType<typeof fakeQueue>;

beforeEach(() => {
  document.body.innerHTML = '<canvas id="arena" tabindex="0"></canvas>';
  preferences.set(initial());
  engine.set(snapshot());
  error.set("");
  focus.set(null);
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  const canvas = document.getElementById("arena") as HTMLCanvasElement;
  router = installInput(canvas, () => {});
  fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
  canvas.focus();
});
afterEach(() => {
  router.dispose();
  cleanup();
  matchQueue.set(null);
  clearTimeout(persistence.timer);
});

const chatMessage = (fields: object) =>
  fake.say({
    type: "chat",
    from: "Rook#0042",
    admin: false,
    text: "hi",
    channel: "public",
    ...fields,
  });

it("opens with Enter, keeps keys from combat while typing, and sends with Enter", async () => {
  const user = userEvent.setup();
  render(<App />);
  const field = screen.getByRole("textbox", { name: "Message to Public" });
  await user.keyboard("{Enter}");
  expect(document.activeElement).toBe(field);
  await user.keyboard("{w>}");
  expect(router.sample()[1]).toBe(0);
  await user.keyboard("{/w}ello{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "chat", channel: "public", text: "wello" });
  expect((field as HTMLInputElement).value).toBe("");
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  // Escape leaves the field with the draft kept, and opens no menu.
  await user.keyboard("{Enter}draft{Escape}");
  expect(interaction.get().panel).toBeNull();
  expect((field as HTMLInputElement).value).toBe("draft");
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
});

it("marks unread tabs, switches channels and shows refusals in words", async () => {
  const user = userEvent.setup();
  render(<App />);
  // All is shown first, with everything in it.
  expect(screen.getByRole("tab", { name: /^All/, selected: true })).toBeTruthy();
  await user.click(screen.getByRole("tab", { name: /^Public/ }));
  chatMessage({ channel: "team", text: "push left", admin: true });
  const team = screen.getByRole("tab", { name: /^Team/ });
  expect(within(team).getByLabelText("1 unread")).toBeTruthy();
  expect(within(screen.getByRole("tab", { name: /^All/ })).getByLabelText("1 unread")).toBeTruthy();
  await user.click(team);
  expect(within(team).queryByLabelText("1 unread")).toBeNull();
  const log = screen.getByRole("log", { name: "Team" });
  expect(log.textContent).toContain("[Team] Rook#0042");
  expect(log.textContent).toContain("push left");
  expect(within(log).getByText("Admin")).toBeTruthy();
  await user.click(screen.getByRole("textbox", { name: "Message to Team" }));
  await user.keyboard("gg{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "chat", channel: "team", text: "gg" });
  fake.say({ type: "refused", code: "not_in_match" });
  expect(screen.getByRole("alert").textContent).toMatch(/not in a match/);
  // A shared code right after a chat post is the chat box's too.
  await user.click(screen.getByRole("button", { name: "OK" }));
  await user.click(screen.getByRole("textbox", { name: "Message to Team" }));
  await user.keyboard("again{Enter}");
  fake.say({ type: "refused", code: "rate_limited" });
  expect(screen.getByRole("alert").textContent).toMatch(/too fast/);
  expect(fake.queue.state.get().notice).toBeNull();
});

it("refuses over-long messages before sending", async () => {
  const user = userEvent.setup();
  render(<App />);
  const field = screen.getByRole("textbox", { name: "Message to Public" });
  await user.click(field);
  fireEvent.change(field, { target: { value: "x".repeat(201) } });
  const before = fake.sent().length;
  await user.keyboard("{Enter}");
  expect(fake.sent()).toHaveLength(before);
  expect(screen.getByText("Messages hold up to 200 characters.")).toBeTruthy();
});

it("whispers with /w in a purple field, opens a tab per partner and ignores players", async () => {
  const user = userEvent.setup();
  render(<App />);
  chatMessage({ text: "hello all" });
  // Whisper from a name: the field starts `/w Rook#0042 ` and turns purple.
  await user.click(screen.getByRole("button", { name: "[Public] Rook#0042: options" }));
  await user.click(screen.getByRole("menuitem", { name: "Whisper" }));
  const field = screen.getByRole("textbox", { name: "Whisper to Rook#0042" }) as HTMLInputElement;
  expect(document.activeElement).toBe(field);
  expect(field.value).toBe("/w Rook#0042 ");
  expect(field.classList.contains("whispering")).toBe(true);
  expect(screen.getByText("Whispering to Rook#0042")).toBeTruthy();
  await user.keyboard("psst{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "whisper", to: "Rook#0042", text: "psst" });
  // Typing it by hand turns purple only once the handle and a space are in.
  await user.keyboard("{Enter}/w Rook#004");
  expect(screen.getByRole("textbox", { name: /Message to/ }).classList.contains("whispering")).toBe(
    false,
  );
  await user.keyboard("2 ");
  expect(screen.getByRole("textbox", { name: "Whisper to Rook#0042" })).toBeTruthy();
  // A typed `/w` wins over the tab; clear it before switching.
  await user.keyboard("{Control>}a{/Control}{Backspace}{Escape}");
  // Replies open the partner's tab (and show in All too).
  chatMessage({ channel: "whisper", from: "Ash#1000", text: "you there?" });
  const tab = screen.getByRole("tab", { name: /^Ash/ });
  expect(screen.getByRole("log", { name: "All" }).textContent).toContain("you there?");
  await user.click(tab);
  expect(screen.getByRole("log", { name: "Ash#1000" }).textContent).toContain("you there?");
  // In a conversation tab, plain text whispers to that player.
  await user.click(screen.getByRole("textbox", { name: "Whisper to Ash#1000" }));
  await user.keyboard("yes{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "whisper", to: "Ash#1000", text: "yes" });
  await user.click(screen.getByRole("button", { name: "Close the conversation with Ash#1000" }));
  expect(screen.queryByRole("tab", { name: /^Ash/ })).toBeNull();
  expect(screen.getByRole("tab", { name: /^All/, selected: true })).toBeTruthy();

  await user.click(screen.getByRole("button", { name: "[Public] Rook#0042: options" }));
  await user.click(screen.getByRole("menuitem", { name: "Ignore" }));
  expect(fake.sent().at(-1)).toEqual({ type: "ignore", handle: "Rook#0042" });
  expect(screen.getByRole("log", { name: "All" }).textContent).not.toContain("hello all");
  // A new connection hears the ignore again.
  act(() => fake.latest().onclose?.());
  fake.queue.connect();
  fake.open();
  expect(fake.sent()).toEqual([
    { type: "hello", guest: "07".repeat(16) },
    { type: "ignore", handle: "Rook#0042" },
  ]);
});

it("undoes an ignore the door refused, but not after a later request", () => {
  fake.queue.ignore("Ghost#0009", true);
  fake.say({ type: "refused", code: "no_such_player" });
  expect(fake.queue.chat.get().ignored).toEqual([]);
  expect(fake.queue.chat.get().error).toBe("no_such_player");
  fake.queue.ignore("Rook#0042", true);
  fake.queue.say("public", "hi");
  fake.say({ type: "refused", code: "rate_limited" });
  expect(fake.queue.chat.get().ignored).toEqual(["Rook#0042"]);
});

it("gives spectators their own tab first", () => {
  engine.set({ ...snapshot(), spectating: true });
  render(<App />);
  expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
    "All",
    "Spectators",
    "Public",
  ]);
  expect(screen.getByRole("textbox", { name: "Message to Spectators" })).toBeTruthy();
});

it("leaves Enter alone where gameplay does not own the keys, and never refocuses on remount", async () => {
  const user = userEvent.setup();
  const { unmount } = render(<App />);
  const field = () => screen.getByRole("textbox", { name: /Message to/ });
  act(() => interaction.set({ ...interaction.get(), layout: true }));
  await user.keyboard("{Enter}");
  expect(document.activeElement).not.toBe(field());
  act(() => interaction.set({ ...interaction.get(), layout: false }));
  await user.keyboard("{Enter}");
  expect(document.activeElement).toBe(field());
  await user.keyboard("{Escape}");
  await waitFor(() => expect(document.activeElement?.id).toBe("arena"));
  unmount();
  render(<App />);
  expect(document.activeElement).not.toBe(field());
  // A hidden box does not open.
  act(() =>
    preferences.set({
      ...preferences.get(),
      profiles: preferences.get().profiles.map((p) => ({
        ...p,
        config: {
          ...p.config,
          frames: { ...p.config.frames, chat: { ...p.config.frames.chat, visible: false } },
        },
      })),
    }),
  );
  await user.keyboard("{Enter}");
  expect(screen.queryByRole("textbox", { name: /Message to/ })).toBeNull();
});

it("leaves other requests' refusals to the queue, even right after a chat post", () => {
  fake.queue.say("public", "hi");
  fake.say({ type: "refused", code: "party_full" });
  expect(fake.queue.state.get().notice).toBe("party_full");
  expect(fake.queue.chat.get().error).toBeNull();
});

it("replays the newest ignores on a new connection, quietly", () => {
  for (const name of ["Aaa", "Bbb", "Ccc", "Ddd", "Eee", "Fff", "Ggg"])
    fake.queue.ignore(`${name}#0001`, true);
  act(() => fake.latest().onclose?.());
  fake.queue.connect();
  fake.open();
  const replayed = fake.sent().filter((m) => m.type === "ignore");
  expect(replayed.map((m) => m.handle)).toEqual([
    "Ccc#0001",
    "Ddd#0001",
    "Eee#0001",
    "Fff#0001",
    "Ggg#0001",
  ]);
  // An ignored player who is offline now: expected, so no error and no undo.
  fake.say({ type: "refused", code: "no_such_player" });
  expect(fake.queue.chat.get().error).toBeNull();
  expect(fake.queue.chat.get().ignored).toHaveLength(7);
});

it("sends from All to the team when seated in a match", async () => {
  const user = userEvent.setup();
  engine.set({ ...snapshot(), networked: true });
  render(<App />);
  expect(screen.getByRole("tab", { name: /^All/, selected: true })).toBeTruthy();
  await user.click(screen.getByRole("textbox", { name: "Message to Team" }));
  await user.keyboard("focus mage{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "chat", channel: "team", text: "focus mage" });
});

it("resizes by its grip down to a minimum, keeping its top-left corner, and by keys", async () => {
  // A chat box the player moved away from the launcher.
  updateConfig((c) => ({ ...c, chatAttached: false }));
  render(<App />);
  const box = screen.getByRole("region", { name: "Chat" });
  const placed = box.closest(".placed") as HTMLElement;
  // jsdom has no layout: report the box where it is drawn, at scale 1.
  const rect = (width: number, height: number) =>
    ({
      left: 20,
      top: 300,
      width,
      height,
      right: 20 + width,
      bottom: 300 + height,
      x: 20,
      y: 300,
    }) as DOMRect;
  placed.getBoundingClientRect = () => rect(360, 210);
  box.getBoundingClientRect = () => rect(360, 210);
  const grip = screen.getByRole("button", { name: /Resize chat \(360 × 210\)/ });
  fireEvent.pointerDown(grip, { button: 0, clientX: 380, clientY: 510, pointerId: 1 });
  fireEvent.pointerMove(grip, { clientX: 520, clientY: 610, pointerId: 1 });
  expect(box.style.width).toBe("500px");
  expect(box.style.height).toBe("310px");
  fireEvent.pointerUp(grip, { pointerId: 1 });
  const saved = preferences.get().profiles.find((p) => p.id === preferences.get().active)!.config;
  expect(saved.chatSize).toEqual({ width: 500, height: 310 });
  // The saved layout puts the top-left corner back at (20, 300) for the new size.
  const travelX = innerWidth - 24 - 500;
  const travelY = innerHeight - 24 - 310;
  expect(saved.frames.chat.x * travelX).toBeCloseTo(8, 0);
  expect(saved.frames.chat.y * travelY).toBeCloseTo(288, 0);
  // Never smaller than the minimum.
  fireEvent.pointerDown(grip, { button: 0, clientX: 500, clientY: 600, pointerId: 2 });
  fireEvent.pointerMove(grip, { clientX: 0, clientY: 0, pointerId: 2 });
  fireEvent.pointerUp(grip, { pointerId: 2 });
  const shrunk = preferences.get().profiles.find((p) => p.id === preferences.get().active)!.config;
  expect(shrunk.chatSize).toEqual({ width: 260, height: 150 });
  // Arrow keys on the grip, for keyboards.
  box.getBoundingClientRect = () => rect(260, 150);
  fireEvent.keyDown(screen.getByRole("button", { name: /Resize chat/ }), { key: "ArrowRight" });
  fireEvent.keyDown(screen.getByRole("button", { name: /Resize chat/ }), {
    key: "ArrowDown",
    shiftKey: true,
  });
  const keyed = preferences.get().profiles.find((p) => p.id === preferences.get().active)!.config;
  expect(keyed.chatSize).toEqual({ width: 270, height: 190 });
});

it("sets width and height in the layout editor when the chat box is selected", async () => {
  render(<App />);
  act(() => {
    interaction.set({ ...interaction.get(), layout: true });
    selectedLayout.set("chat");
  });
  fireEvent.change(screen.getByRole("slider", { name: /Width/ }), { target: { value: "500" } });
  fireEvent.change(screen.getByRole("slider", { name: /Height/ }), { target: { value: "320" } });
  expect(screen.getByRole("region", { name: "Chat" }).style.width).toBe("500px");
  expect(screen.getByRole("region", { name: "Chat" }).style.height).toBe("320px");
  fireEvent.click(screen.getByRole("button", { name: "Reset selected" }));
  expect(screen.getByRole("region", { name: "Chat" }).style.width).toBe("360px");
});

it("keeps a drag within the screen, cancels a lost drag, and hides the grip while editing", () => {
  const width = innerWidth;
  const height = innerHeight;
  Object.assign(window, { innerWidth: 800, innerHeight: 600 });
  try {
    render(<App />);
    const box = screen.getByRole("region", { name: "Chat" });
    const placed = box.closest(".placed") as HTMLElement;
    const rect = {
      left: 20,
      top: 300,
      width: 360,
      height: 210,
      right: 380,
      bottom: 510,
      x: 20,
      y: 300,
    } as DOMRect;
    placed.getBoundingClientRect = () => rect;
    box.getBoundingClientRect = () => rect;
    const grip = screen.getByRole("button", { name: /Resize chat/ });
    fireEvent.pointerDown(grip, { button: 0, clientX: 380, clientY: 510, pointerId: 1 });
    fireEvent.pointerMove(grip, { clientX: 2000, clientY: 2000, pointerId: 1 });
    // 800×600 leaves 776×576 to use.
    expect(box.style.width).toBe("776px");
    expect(box.style.height).toBe("576px");
    fireEvent.lostPointerCapture(grip, { pointerId: 1 });
    expect(box.style.width).toBe("360px");
    act(() => interaction.set({ ...interaction.get(), layout: true }));
    expect(screen.queryByRole("button", { name: /Resize chat/ })).toBeNull();
  } finally {
    Object.assign(window, { innerWidth: width, innerHeight: height });
    act(() => interaction.set({ ...interaction.get(), layout: false }));
  }
});

it("sits beside the launcher by default, filling the width to the action bar, until moved", async () => {
  // jsdom has no layout: report where the launcher and action bar are drawn.
  const rect = (left: number, top: number, width: number, height: number) =>
    ({
      left,
      top,
      width,
      height,
      right: left + width,
      bottom: top + height,
      x: left,
      y: top,
    }) as DOMRect;
  const original = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    if (this.querySelector?.(".launcher")) return rect(12, innerHeight - 162, 34, 150);
    if (this.querySelector?.(".action-bar") || this.classList?.contains("action-bar"))
      return rect(520, innerHeight - 80, 824, 64);
    return original.call(this);
  };
  try {
    render(<App />);
    await act(async () => {
      await new Promise((done) => requestAnimationFrame(() => done(null)));
    });
    const box = screen.getByRole("region", { name: "Chat" });
    // From 8 px right of the launcher (46) to 8 px left of the bar (520).
    expect(box.style.width).toBe(`${520 - 8 - 54}px`);
    // Layout editing moves it: it detaches.
    act(() => interaction.set({ ...interaction.get(), layout: true }));
    const edit = screen.getByRole("button", { name: "Edit chat" });
    fireEvent.pointerDown(edit, { button: 0, clientX: 100, clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(edit, { clientX: 140, clientY: 100, pointerId: 1 });
    fireEvent.pointerUp(edit, { pointerId: 1 });
    const saved = preferences.get().profiles.find((p) => p.id === preferences.get().active)!.config;
    expect(saved.chatAttached).toBe(false);
    // It keeps the width it showed beside the launcher.
    expect(saved.chatSize.width).toBe(520 - 8 - 54);
  } finally {
    Element.prototype.getBoundingClientRect = original;
    act(() => interaction.set({ ...interaction.get(), layout: false }));
  }
});

it("follows the launcher when it moves", async () => {
  let launcherLeft = 12;
  const rect = (left: number, top: number, width: number, height: number) =>
    ({
      left,
      top,
      width,
      height,
      right: left + width,
      bottom: top + height,
      x: left,
      y: top,
    }) as DOMRect;
  const original = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    if (this.querySelector?.(".launcher")) return rect(launcherLeft, innerHeight - 162, 34, 150);
    if (this.querySelector?.(".action-bar") || this.classList?.contains("action-bar"))
      return rect(700, innerHeight - 80, 824, 64);
    return original.call(this);
  };
  const frame = () =>
    act(async () => {
      await new Promise((done) => requestAnimationFrame(() => done(null)));
    });
  try {
    render(<App />);
    await frame();
    const box = screen.getByRole("region", { name: "Chat" });
    expect(box.style.width).toBe(`${700 - 8 - 54}px`);
    // The launcher moves 100 px right (its frame's style changes): the chat
    // re-measures and narrows to keep filling the space.
    launcherLeft = 112;
    const launcher = document.querySelector(".launcher")!.closest(".placed") as HTMLElement;
    launcher.style.left = "112px";
    await frame();
    await frame();
    expect(box.style.width).toBe(`${700 - 8 - 154}px`);
  } finally {
    Element.prototype.getBoundingClientRect = original;
  }
});

it("keeps // lines in a conversation private, and Whisper keeps a draft", async () => {
  const user = userEvent.setup();
  render(<App />);
  chatMessage({ channel: "whisper", from: "Ash#1000", text: "hi" });
  await user.click(screen.getByRole("tab", { name: /^Ash/ }));
  await user.click(screen.getByRole("textbox", { name: "Whisper to Ash#1000" }));
  await user.keyboard("//shrug{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "whisper", to: "Ash#1000", text: "/shrug" });
  // A real command still runs from the conversation tab.
  await user.click(screen.getByRole("textbox", { name: "Whisper to Ash#1000" }));
  await user.keyboard("/help{Enter}");
  expect(fake.sent().at(-1)).toEqual({ type: "whisper", to: "Ash#1000", text: "/shrug" });
  // Whisper from a name puts `/w Name#0000 ` in front of a message already typed.
  await user.click(screen.getByRole("tab", { name: /^All/ }));
  await user.click(screen.getByRole("textbox", { name: /Message to/ }));
  await user.keyboard("half typed{Escape}");
  await user.click(screen.getAllByRole("button", { name: "Ash#1000 whispers: options" })[0]);
  await user.click(screen.getByRole("menuitem", { name: "Whisper" }));
  expect(
    (screen.getByRole("textbox", { name: "Whisper to Ash#1000" }) as HTMLInputElement).value,
  ).toBe("/w Ash#1000 half typed");
});
