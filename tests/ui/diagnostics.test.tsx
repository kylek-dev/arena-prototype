import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Diagnostics } from "../../web/src/features/diagnostics/Diagnostics";
import { FrameTiming } from "../../web/src/domain/frame-timing";
import { diagnosticsVisible, frameTiming } from "../../web/src/runtime/diagnostics";
import { engine } from "../../web/src/state/session";
import { snapshot } from "./fixtures";

vi.mock("../../web/src/runtime/diagnostics", async (original) => ({
  ...(await original<typeof import("../../web/src/runtime/diagnostics")>()),
  debugEnabled: true,
}));
beforeEach(() => {
  diagnosticsVisible.set(true);
  engine.set(snapshot());
  const timing = new FrameTiming();
  timing.add(100, 50);
  frameTiming.set({ ...timing.summary(100), engine: timing.summary(100) });
});
afterEach(cleanup);

it("shows compact frame metrics and graph without mounting the large snapshot", () => {
  const { container } = render(<Diagnostics />);
  expect(screen.getByText("20 browser FPS")).toBeTruthy();
  expect(screen.getByText("20 engine Hz")).toBeTruthy();
  expect(screen.getByText("P99")).toBeTruthy();
  expect(screen.getByText("Max")).toBeTruthy();
  expect(screen.getByRole("img", { name: /Frame-time peaks/ })).toBeTruthy();
  expect(container.querySelector("#debug-state")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Show engine snapshot" }));
  expect(container.querySelector("#debug-state")?.textContent).toContain("practice-player");
  fireEvent.click(screen.getByRole("button", { name: "Hide engine snapshot" }));
  expect(container.querySelector("#debug-state")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Reset samples" }));
  expect(screen.getByText("— browser FPS")).toBeTruthy();
  expect(screen.getByText("— engine Hz")).toBeTruthy();
  act(() => diagnosticsVisible.set(false));
  expect(screen.queryByRole("complementary")).toBeNull();
});
