import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { CenterPanel } from "../../web/src/features/center/CenterPanel";
import { InvitePrompt } from "../../web/src/features/friends/InvitePrompt";
import { MatchReady } from "../../web/src/features/queue/MatchReady";
import { QueueBar } from "../../web/src/features/queue/QueueBar";
import { matchQueue, networkLink } from "../../web/src/state/network";
import { createStore } from "../../web/src/state/store";
import { interaction } from "../../web/src/state/session";
import type { LinkState } from "../../web/src/domain/network";
import { fakeQueue } from "./fake-queue";

let fake: ReturnType<typeof fakeQueue>;
beforeEach(() => {
  fake = fakeQueue();
  matchQueue.set(fake.queue);
  fake.queue.connect();
  fake.open();
});
afterEach(() => {
  cleanup();
  matchQueue.set(null);
  networkLink.set(null);
});

const panel = () =>
  render(
    <CenterPanel>
      <MatchReady />
      <QueueBar />
      <InvitePrompt />
    </CenterPanel>,
  );

it("stacks the search, a found match and invitations in the center outside a match", () => {
  panel();
  const center = document.querySelector(".center-panel") as HTMLElement;
  fake.say({ type: "queued", mode: "2v2", waiting: 3, waited_ms: 5_000, expected_ms: null });
  const search = screen.getByRole("status", { name: "Queue" });
  expect(center.contains(search)).toBe(true);
  expect(search.classList.contains("centered")).toBe(true);
  fake.say({ type: "invite", from: "Ash#1000", kind: "party", code: "K7P2QX" });
  const invite = screen.getByRole("group", { name: "Invite from Ash#1000" });
  expect(center.contains(invite)).toBe(true);
  expect(invite.closest(".invite-prompts")?.classList.contains("centered")).toBe(true);
  // The party code never shows.
  expect(center.textContent).not.toContain("K7P2QX");
  fake.say({
    type: "ready_check",
    mode: "2v2",
    accepted: 0,
    total: 4,
    you: false,
    remaining_ms: 15_000,
  });
  const ready = screen.getByRole("alertdialog");
  expect(center.contains(ready)).toBe(true);
  expect(document.querySelector(".match-ready-backdrop")).toBeNull();
  expect(within(ready).getByRole("button", { name: "Accept" })).toBeTruthy();
});

it("keeps invitations at the right edge during a match", () => {
  const state = createStore<LinkState>({
    phase: "open",
    attempt: 0,
    ended: null,
    status: null,
  } as unknown as LinkState);
  networkLink.set({ state } as unknown as ReturnType<typeof networkLink.get>);
  panel();
  fake.say({ type: "invite", from: "Ash#1000", kind: "lobby", code: "K7P2QX" });
  const invite = screen.getByRole("group", { name: "Invite from Ash#1000" });
  expect(invite.closest(".invite-prompts")?.classList.contains("centered")).toBe(false);
});

it("keeps the queue at the top and invitations at the right edge while a centered window is open", () => {
  interaction.set({ ...interaction.get(), panel: "settings" });
  try {
    panel();
    fake.say({ type: "queued", mode: "2v2", waiting: 3, waited_ms: 5_000, expected_ms: null });
    expect(screen.getByRole("status", { name: "Queue" }).classList.contains("centered")).toBe(
      false,
    );
    fake.say({ type: "invite", from: "Ash#1000", kind: "party", code: "K7P2QX" });
    const invite = screen.getByRole("group", { name: "Invite from Ash#1000" });
    expect(invite.closest(".invite-prompts")?.classList.contains("centered")).toBe(false);
  } finally {
    interaction.set({ ...interaction.get(), panel: null });
  }
});
