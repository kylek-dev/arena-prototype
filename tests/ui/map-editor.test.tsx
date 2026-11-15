import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import colosseumText from "../../assets/maps/colosseum.json?raw";
import catalogText from "../../assets/props/catalog.json?raw";
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
      if (url.endsWith("assets/maps/arena-two.json"))
        return new Response(JSON.stringify({ ...JSON.parse(colosseumText), name: "Arena Two" }));
      if (url.endsWith("assets/props/catalog.json")) return new Response(catalogText);
      if (url.endsWith("__map-editor/maps"))
        return Response.json({
          maps: [
            { id: "arena-two", name: "Arena Two" },
            { id: "colosseum", name: "Colosseum" },
          ],
        });
      const save = url.match(/__map-editor\/maps\/([a-z0-9-]+)\.json$/);
      if (save) return Response.json({ saved: `assets/maps/${save[1]}.json` });
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
  // Catalog and map list load when the editor opens.
  await act(async () => {});
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
  const save = screen.getByRole("button", { name: "Save as colosseum.json" });
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
  expect(screen.queryByRole("button", { name: "Save as colosseum.json" })).toBeNull();
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

it("places, drags, turns and edits props, and changes the ground", async () => {
  await openEditor();
  const { plan, at } = layOutPlan();
  const add = within(screen.getByRole("region", { name: "Add prop" }));
  fireEvent.change(add.getByRole("combobox", { name: "Prop model" }), {
    target: { value: "Bench" },
  });
  fireEvent.click(add.getByRole("button", { name: "Add prop" }));
  expect(sentMap().map.props).toEqual([
    { model: "Bench", x: 0, z: 0, y: 0, rotation: 0, scale: 1 },
  ]);
  const inspector = screen.getByRole("region", { name: "Selected prop" });
  expect(inspector.textContent).toContain("Prop 1 · Bench");
  expect(items().getByRole("button", { name: "Prop 1 · Bench" })).toBeTruthy();

  const bench = within(plan).getByRole("button", { name: "Prop 1 (Bench)" });
  fireEvent.pointerDown(bench, { button: 0, pointerId: 1, ...at(0, 0) });
  fireEvent.pointerMove(plan, { pointerId: 1, ...at(6.2, -3.9) });
  fireEvent.pointerUp(plan, { pointerId: 1 });
  expect(sentMap().map.props[0]).toMatchObject({ x: 6, z: -4 });

  fireEvent.keyDown(plan, { key: "r" });
  fireEvent.keyDown(plan, { key: "r" });
  expect(sentMap().map.props[0].rotation).toBe(30);
  fireEvent.keyDown(plan, { key: "R", shiftKey: true });
  expect(sentMap().map.props[0].rotation).toBe(15);
  fireEvent.click(within(inspector).getByRole("button", { name: "Turn 90°" }));
  expect(sentMap().map.props[0].rotation).toBe(105);
  const scale = within(inspector).getByRole("spinbutton", { name: /Scale/ });
  fireEvent.change(scale, { target: { value: "1.5" } });
  fireEvent.keyDown(scale, { key: "Enter" });
  expect(sentMap().map.props[0].scale).toBe(1.5);
  fireEvent.change(within(inspector).getByRole("combobox", { name: "Model" }), {
    target: { value: "Barrel" },
  });
  expect(sentMap().map.props[0]).toMatchObject({ model: "Barrel", x: 6, rotation: 105 });
  fireEvent.keyDown(plan, { key: "d", ctrlKey: true });
  expect(sentMap().map.props).toHaveLength(2);
  expect(sentMap().map.props[1]).toMatchObject({ model: "Barrel", x: 7.5 });
  fireEvent.keyDown(plan, { key: "Delete" });
  expect(sentMap().map.props).toHaveLength(1);

  fireEvent.click(items().getByRole("button", { name: "Map & venue" }));
  fireEvent.change(screen.getByRole("combobox", { name: "Ground" }), {
    target: { value: "snow" },
  });
  expect(sentMap().map.ground).toBe("snow");
  expect(plan.querySelector(".plan-floor")?.getAttribute("class")).toContain("ground-snow");
});

it("creates a new map file, saves it under its id and lists maps to open", async () => {
  await openEditor();
  const open = screen.getByRole("combobox", { name: "Open map" }) as HTMLSelectElement;
  expect([...open.options].map((o) => o.value)).toEqual(["arena-two", "colosseum"]);
  fireEvent.click(screen.getByRole("button", { name: "New map…" }));
  const form = within(screen.getByRole("region", { name: "New map" }));
  fireEvent.change(form.getByRole("textbox", { name: "Name" }), {
    target: { value: "Arena Two" },
  });
  fireEvent.click(form.getByRole("button", { name: "Create map" }));
  expect(form.getByRole("alert").textContent).toContain('"arena-two" already exists');
  fireEvent.change(form.getByRole("textbox", { name: "Name" }), {
    target: { value: "Frozen Pit!" },
  });
  fireEvent.change(form.getByRole("combobox", { name: "Ground" }), {
    target: { value: "snow" },
  });
  fireEvent.click(form.getByRole("button", { name: "Create map" }));
  expect(screen.queryByRole("region", { name: "New map" })).toBeNull();
  const { map } = sentMap();
  expect(map).toMatchObject({ name: "Frozen Pit!", ground: "snow", pillars: [], props: [] });
  expect(map.rooms.map((r: { team: string }) => r.team)).toEqual(["friendly", "opposing"]);
  // A new file is unsaved and has no undo back into the previous map.
  expect(screen.getByRole("button", { name: "Undo" })).toHaveProperty("disabled", true);
  expect(screen.getByText("Unsaved changes.", { exact: false })).toBeTruthy();
  expect(screen.getByText("New map frozen-pit: save it to keep it.")).toBeTruthy();
  verdict();
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "Save as frozen-pit.json" })),
  );
  const put = calls.find((c) => c.init?.method === "PUT")!;
  expect(put.url).toMatch(/__map-editor\/maps\/frozen-pit\.json$/);
  expect(screen.getByText("Saved assets/maps/frozen-pit.json.")).toBeTruthy();
  expect(localStorage.getItem("arena.map.v1")).toBe("frozen-pit");

  // Opening another map replaces the editor's map and history.
  await act(async () => {
    fireEvent.change(screen.getByRole("combobox", { name: "Open map" }), {
      target: { value: "arena-two" },
    });
  });
  await waitFor(() => expect(sentMap().map.name).toBe("Arena Two"));
  expect(screen.getByRole("button", { name: "Save as arena-two.json" })).toBeTruthy();

  // With unsaved changes, opening asks first.
  fireEvent.click(screen.getByRole("button", { name: "Add pillar" }));
  fireEvent.change(screen.getByRole("combobox", { name: "Open map" }), {
    target: { value: "colosseum" },
  });
  expect(sentMap().map.name).toBe("Arena Two");
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Open colosseum" })));
  await waitFor(() => expect(sentMap().map.name).toBe("Colosseum"));
  localStorage.clear();
});

it("applies a typed inspector value when the plan is clicked, and spinner steps at once", async () => {
  await openEditor();
  const { plan } = layOutPlan();
  fireEvent.click(items().getByRole("button", { name: "Pillar 1" }));
  const pillar = () => within(screen.getByRole("region", { name: "Selected pillar" }));
  const x = pillar().getByRole("spinbutton", { name: /Centre X/ });
  x.focus();
  fireEvent.change(x, { target: { value: "-25" } });
  expect(sentMap().map.pillars[0].x).toBe(-21);
  // Clicking empty plan clears the selection; the edit must not be lost.
  fireEvent.pointerDown(plan, { button: 0 });
  expect(sentMap().map.pillars[0].x).toBe(-25);
  expect(screen.getByRole("region", { name: "Map settings" })).toBeTruthy();

  fireEvent.click(items().getByRole("button", { name: "Pillar 2" }));
  const z = pillar().getByRole("spinbutton", { name: /Centre Z/ });
  // A spinner click changes the value and releases the pointer on the field.
  fireEvent.change(z, { target: { value: "-14.5" } });
  fireEvent.pointerUp(z);
  expect(sentMap().map.pillars[1].z).toBe(-14.5);
  fireEvent.change(z, { target: { value: "-14" } });
  fireEvent.keyUp(z, { key: "ArrowUp" });
  expect(sentMap().map.pillars[1].z).toBe(-14);
});

it("gives every prop a fixed-size click target, however small the model", async () => {
  await openEditor();
  const add = within(screen.getByRole("region", { name: "Add prop" }));
  fireEvent.change(add.getByRole("combobox", { name: "Prop model" }), {
    target: { value: "Coin" },
  });
  fireEvent.click(add.getByRole("button", { name: "Add prop" }));
  fireEvent.click(items().getByRole("button", { name: "Map & venue" }));
  const plan = screen.getByRole("group", { name: "Map plan (north is up)" });
  const coin = within(plan).getByRole("button", { name: "Prop 1 (Coin)" });
  const hit = coin.querySelector(".plan-prop-hit")!;
  // jsdom has no layout, so the plan keeps its default 4 px per metre.
  expect(Number(hit.getAttribute("r"))).toBeCloseTo(9 / 4);
  fireEvent.pointerDown(hit, { button: 0, pointerId: 1, clientX: 0, clientY: 0 });
  expect(screen.getByRole("region", { name: "Selected prop" }).textContent).toContain("Coin");
});
