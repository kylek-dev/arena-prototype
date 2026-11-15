import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import colosseumText from "../../assets/maps/colosseum.json?raw";
import { MapEditor } from "../../web/src/features/map-editor/MapEditor";
import { GameMenu } from "../../web/src/features/menu/GameMenu";
import {
  loadMap,
  mapEditorVisible,
  onMapStatus,
  readMap,
  resetMapEditor,
} from "../../web/src/state/map-editor";
import { interaction, setPanel } from "../../web/src/state/session";

type Call = { url: string; init?: RequestInit };
let calls: Call[] = [];
let writable = true;

beforeEach(() => {
  resetMapEditor();
  setPanel(null);
  calls = [];
  writable = true;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: URL | string, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, init });
      if (url.endsWith("assets/maps/colosseum.json")) return new Response(colosseumText);
      if (url.endsWith("__map-editor/status"))
        return writable ? Response.json({ writable: true }) : new Response("", { status: 404 });
      if (url.endsWith("__map-editor/maps/colosseum.json"))
        return Response.json({ saved: "assets/maps/colosseum.json" });
      return new Response("", { status: 404 });
    }),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const sentMap = () => {
  const [revision, json] = readMap() as [number, string];
  return { revision, map: JSON.parse(json) };
};
/** Rust's answer for the latest revision. */
const verdict = (errors: string[] = []) =>
  act(() => onMapStatus({ revision: sentMap().revision, errors }));

async function openEditor() {
  await act(() => loadMap());
  setPanel("menu");
  render(
    <>
      <GameMenu />
      <MapEditor />
    </>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Map editor" }));
  expect(interaction.get().panel).toBeNull();
  return screen.getByRole("complementary", { name: "Map editor" });
}

const items = () => within(screen.getByRole("navigation", { name: "Map items" }));

/** The colosseum plan is 161 × 137 m; draw it 1 px per metre from the origin. */
function layOutPlan() {
  const plan = screen.getByRole("group", { name: "Map plan (north is up)" });
  plan.getBoundingClientRect = () => new DOMRect(0, 0, 161, 137);
  return { plan, at: (x: number, z: number) => ({ clientX: x + 80.5, clientY: z + 68.5 }) };
}

it("loads the map file at startup and sends it to the arena as revision 1", async () => {
  expect(readMap()).toEqual([]);
  await act(() => loadMap());
  expect(calls[0].url).toMatch(/assets\/maps\/colosseum\.json$/);
  expect(sentMap()).toEqual({ revision: 1, map: JSON.parse(colosseumText) });
});

it("opens from the menu, draws every item and reports the arena's verdict", async () => {
  await openEditor();
  const plan = within(screen.getByRole("group", { name: "Map plan (north is up)" }));
  expect(plan.getAllByRole("button", { name: /^Pillar \d$/ })).toHaveLength(4);
  expect(plan.getByRole("button", { name: "Room 1 (friendly, south)" })).toBeTruthy();
  expect(plan.getAllByRole("button", { name: /spawn \d$/ })).toHaveLength(6);
  expect(screen.getByRole("status").textContent).toContain("Checking map");
  verdict(["Room 1's ceiling must be at least 5.7 m (three character heights)."]);
  expect(screen.getByRole("list", { name: "Map problems" }).textContent).toContain("5.7 m");
  verdict();
  expect(screen.getByRole("status").textContent).toContain("Valid · live in the arena.");
  // Escape clears a selection first, then closes.
  fireEvent.click(items().getByRole("button", { name: "Pillar 2" }));
  fireEvent.keyDown(screen.getByRole("group", { name: "Map plan (north is up)" }), {
    key: "Escape",
  });
  expect(screen.getByRole("region", { name: "Map settings" })).toBeTruthy();
  fireEvent.keyDown(screen.getByRole("group", { name: "Map plan (north is up)" }), {
    key: "Escape",
  });
  expect(mapEditorVisible.get()).toBe(false);
});

it("previews drags in the plan and commits one revision on release, with undo and redo", async () => {
  await openEditor();
  const { plan, at } = layOutPlan();
  const pillar = within(plan).getByRole("button", { name: "Pillar 1" });
  fireEvent.pointerDown(pillar, { button: 0, pointerId: 1, ...at(-21, -15) });
  fireEvent.pointerMove(plan, { pointerId: 1, ...at(-15.8, -12) });
  fireEvent.pointerMove(plan, { pointerId: 1, ...at(-10.9, -15.2) });
  // Not sent to the arena until release.
  expect(sentMap().revision).toBe(1);
  expect(pillar.getAttribute("x")).toBe("-15.5");
  fireEvent.pointerUp(plan, { pointerId: 1 });
  expect(sentMap().revision).toBe(2);
  expect(sentMap().map.pillars[0]).toMatchObject({ x: -11, z: -15 });
  expect(screen.getByRole("status").textContent).toContain("Unsaved changes");

  // A click without movement only selects.
  fireEvent.pointerDown(screen.getByRole("button", { name: "Room 2 spawn 1" }), {
    button: 0,
    ...at(0, -47),
  });
  fireEvent.pointerUp(plan, { pointerId: 1 });
  expect(sentMap().revision).toBe(2);
  expect(screen.getByRole("region", { name: "Selected spawn" })).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  expect([sentMap().revision, sentMap().map.pillars[0].x]).toEqual([3, -21]);
  fireEvent.keyDown(plan, { key: "z", ctrlKey: true, shiftKey: true });
  expect([sentMap().revision, sentMap().map.pillars[0].x]).toEqual([4, -11]);
});

it("edits through the inspector and keyboard and keeps the selection valid", async () => {
  await openEditor();
  const plan = screen.getByRole("group", { name: "Map plan (north is up)" });
  fireEvent.click(items().getByRole("button", { name: "Room 1 · friendly" }));
  const room = screen.getByRole("region", { name: "Selected room" });
  const depth = within(room).getByRole("spinbutton", { name: /Depth/ });
  fireEvent.change(depth, { target: { value: "14" } });
  expect(sentMap().revision).toBe(1);
  fireEvent.keyDown(depth, { key: "Enter" });
  expect(sentMap().map.rooms[0].depth).toBe(14);
  // Typing in a field never triggers editor shortcuts.
  fireEvent.keyDown(depth, { key: "Delete" });
  expect(sentMap().map.rooms).toHaveLength(2);
  fireEvent.change(within(room).getByRole("combobox", { name: "Floor edge" }), {
    target: { value: "west" },
  });
  expect(sentMap().map.rooms[0].edge).toBe("west");

  fireEvent.click(screen.getByRole("button", { name: "Add pillar" }));
  expect(sentMap().map.pillars).toHaveLength(5);
  fireEvent.keyDown(plan, { key: "ArrowRight", shiftKey: true });
  expect(sentMap().map.pillars[4]).toMatchObject({ x: 2, z: 0 });
  fireEvent.keyDown(plan, { key: "d", ctrlKey: true });
  expect(sentMap().map.pillars).toHaveLength(6);
  fireEvent.keyDown(plan, { key: "Delete" });
  expect(sentMap().map.pillars).toHaveLength(5);
  // The deleted pillar's selection is dropped, so a second Delete does nothing.
  expect(screen.getByRole("region", { name: "Map settings" })).toBeTruthy();
  fireEvent.keyDown(plan, { key: "Delete" });
  expect(sentMap().map.pillars).toHaveLength(5);

  fireEvent.click(screen.getByRole("button", { name: "Add opposing room" }));
  expect(sentMap().map.rooms[2]).toMatchObject({ team: "opposing", edge: "north" });
});

it("saves only valid, changed maps through the local server, and falls back to download", async () => {
  await openEditor();
  await act(async () => {});
  const save = screen.getByRole("button", { name: "Save to assets/maps" });
  verdict();
  expect(save).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", { name: "Add pillar" }));
  verdict(["A pillar blocks room 1's opening."]);
  expect(save).toHaveProperty("disabled", true);
  verdict();
  expect(save).toHaveProperty("disabled", false);
  await act(async () => fireEvent.click(save));
  const put = calls.find((c) => c.init?.method === "PUT")!;
  expect(put.url).toMatch(/__map-editor\/maps\/colosseum\.json$/);
  expect(put.init?.headers).toEqual({ "Content-Type": "application/json" });
  expect(JSON.parse(String(put.init?.body)).pillars).toHaveLength(5);
  expect(String(put.init?.body).endsWith("}\n")).toBe(true);
  expect(screen.getByText("Saved assets/maps/colosseum.json.")).toBeTruthy();
  expect(save).toHaveProperty("disabled", true);

  cleanup();
  resetMapEditor();
  writable = false;
  await openEditor();
  await act(async () => {});
  expect(screen.queryByRole("button", { name: "Save to assets/maps" })).toBeNull();
  expect(screen.getByRole("button", { name: "Download" })).toBeTruthy();
  expect(screen.getByText(/Saving needs the local dev server/)).toBeTruthy();
});

it("imports a file as an undoable change and rejects files it cannot read", async () => {
  await openEditor();
  const input = screen.getByLabelText("Import map file") as HTMLInputElement;
  const edited = JSON.parse(colosseumText);
  edited.name = "Imported";
  const upload = (text: string) =>
    act(async () => {
      fireEvent.change(input, { target: { files: [new File([text], "arena.json")] } });
    });
  await upload(JSON.stringify(edited));
  await waitFor(() => expect(screen.getByText("Imported arena.json.")).toBeTruthy());
  expect(sentMap().map.name).toBe("Imported");
  await upload('{"format": 9}');
  await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("format 9"));
  expect(sentMap().map.name).toBe("Imported");
  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  expect(sentMap().map.name).toBe("Colosseum");
});
