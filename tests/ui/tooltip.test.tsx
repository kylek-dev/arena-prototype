import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { Hint, TooltipCard, TooltipProvider } from "../../web/src/ui/Tooltip";
import { SpellDetails } from "../../web/src/features/abilities/SpellDetails";
import { ActionSlot } from "../../web/src/features/action-bars/ActionSlot";
import { Spellbook } from "../../web/src/features/spellbook/Spellbook";
import { initial } from "../../web/src/domain/preferences/schema";
import { preferences } from "../../web/src/state/config";
import { engine, interaction } from "../../web/src/state/session";
import { snapshot } from "./fixtures";

afterEach(cleanup);

it("supports generic content on focus, describes the trigger and dismisses with Escape", async () => {
  const user = userEvent.setup();
  render(
    <TooltipProvider>
      <Hint text={<TooltipCard title="Health" description="Remaining health." />}>
        <button>Inspect</button>
      </Hint>
    </TooltipProvider>,
  );
  await user.tab();
  const tooltip = await screen.findByRole("tooltip");
  expect(tooltip.textContent).toContain("Remaining health.");
  expect(screen.getByRole("button").getAttribute("aria-describedby")).toBe(tooltip.id);
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("tooltip")).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole("button"));
});

it("opens on hover, stays hoverable and closes when tooltips are suppressed", async () => {
  const user = userEvent.setup();
  function Example({ disabled = false }) {
    return (
      <TooltipProvider disabled={disabled} delayDuration={0}>
        <Hint text="Shared help">
          <button>Inspect</button>
        </Hint>
      </TooltipProvider>
    );
  }
  const { rerender } = render(<Example />);
  await user.hover(screen.getByRole("button"));
  expect(await screen.findByRole("tooltip")).toBeTruthy();
  // Hover the visible content, not Radix's hidden accessible description.
  await user.hover(screen.getByRole("tooltip").parentElement!);
  expect(screen.getByRole("tooltip")).toBeTruthy();
  rerender(<Example disabled />);
  expect(screen.queryByRole("tooltip")).toBeNull();
  await user.unhover(screen.getByRole("button"));
  await user.hover(screen.getByRole("button"));
  expect(screen.queryByRole("tooltip")).toBeNull();
  rerender(<Example />);
  expect(screen.queryByRole("tooltip")).toBeNull();
});

it("uses rule-owned spell facts and does not invent values while loading", () => {
  const state = snapshot();
  state.pulse = { damage: 37, range: 9, cooldown: 2.5 };
  const { rerender } = render(<SpellDetails state={state} instruction="Drag to place." />);
  expect(screen.getByText("37")).toBeTruthy();
  expect(screen.getByText("9 m")).toBeTruthy();
  expect(screen.getByText("2.5 s")).toBeTruthy();
  rerender(<SpellDetails state={null} instruction="Drag to place." />);
  expect(screen.getByText("Loading arena")).toBeTruthy();
  expect(screen.queryByText("Damage")).toBeNull();
});

it("shares spell details across slots and the spellbook, with live status and empty-slot help", async () => {
  const user = userEvent.setup();
  preferences.set(initial());
  engine.set(snapshot());
  interaction.set({ panel: null, quick: false, layout: false, hovered: null, capture: null });
  render(
    <TooltipProvider>
      <ActionSlot slot={{ id: "bar-1-s1", ability: "pulse" }} number={1} />
      <ActionSlot slot={{ id: "bar-1-s2", ability: null }} number={2} />
      <Spellbook />
    </TooltipProvider>,
  );
  await user.tab();
  let tooltip = await screen.findByRole("tooltip");
  expect(within(tooltip).getByText("Damage")).toBeTruthy();
  expect(within(tooltip).getByText("1")).toBeTruthy();
  act(() => engine.set(snapshot({ cooldown: 0.6 })));
  expect(within(tooltip).getByText("Recharging · 0.6 s")).toBeTruthy();
  await user.tab();
  tooltip = await screen.findByRole("tooltip");
  expect(within(tooltip).getByText("Empty slot")).toBeTruthy();
  expect(within(tooltip).queryByText("Damage")).toBeNull();
  expect(within(tooltip).getByText("Unbound")).toBeTruthy();
  await user.tab();
  tooltip = await screen.findByRole("tooltip");
  expect(within(tooltip).getByText("Damage")).toBeTruthy();
  expect(within(tooltip).getByText(/never casts/)).toBeTruthy();
});
