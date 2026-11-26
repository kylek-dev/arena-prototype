// The closed palette (docs/palette.json): bindings stay generated from it, and
// the web client and lighting data use nothing else.
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import test from "node:test";
import { load, outputs, root, swatches, targets, validate } from "../../scripts/palette.mjs";

const spec = load();
const hexes = new Set(swatches(spec).map((s) => s.hex));
const generated = new Set(Object.values(targets));
const read = (path) => readFileSync(join(root, path), "utf8");

function files(dir) {
  return readdirSync(join(root, dir), { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return files(path);
    return /\.(css|ts|tsx)$/.test(entry.name) ? [path] : [];
  });
}
const webSources = files("web/src").filter((path) => !generated.has(relative(".", path)));

/** Lines of `text` matching `pattern`, as "path:line: text". */
function offences(path, text, pattern) {
  return text
    .split("\n")
    .flatMap((line, i) => (pattern.test(line) ? [`${path}:${i + 1}: ${line.trim()}`] : []));
}

test("generated bindings match docs/palette.json", () => {
  for (const [path, text] of Object.entries(outputs(spec)))
    assert.equal(read(path), text, `${path} is stale; run node scripts/palette.mjs`);
});

test("palette validation rejects duplicates and unknown references", () => {
  const copy = () => structuredClone(spec);
  const duplicate = copy();
  duplicate.families[0].swatches.push({ ...duplicate.families[0].swatches[0], id: "night-99" });
  assert.throws(() => validate(duplicate), /repeats/);
  const unknown = copy();
  unknown.radix.scales.slate.solid[0] = "nope-1";
  assert.throws(() => validate(unknown), /unknown swatch/);
});

test("web sources use palette variables instead of color literals", () => {
  // A # right after a letter or digit is never a color: player handles
  // such as Name#0000 are text.
  const literal = /(?<![\w])#[0-9a-fA-F]{3,8}\b|\b(rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/;
  // CSS named colors other than transparent/currentColor (checked in CSS only,
  // where they cannot be prose).
  const named =
    /:[^;]*(?<![-\w])(white|black|red|green|blue|yellow|orange|purple|pink|gray|grey|silver|gold|navy|teal|cyan|magenta)(?![-\w])/;
  const found = webSources.flatMap((path) => [
    ...offences(path, read(path), literal),
    ...(path.endsWith(".css")
      ? offences(path, read(path).replaceAll("white-space", ""), named)
      : []),
  ]);
  assert.deepEqual(found, [], "use var(--pal-*) or palette ids from web/src/domain/palette.ts");
});

test("only palette-driven Radix scales and color props are used", () => {
  const driven = new Set([
    ...Object.keys(spec.radix.scales),
    ...["gray", "accent", "focus", "black", "white"],
  ]);
  // Radix Themes color scales; other numbered tokens (radius, space, shadow)
  // are not colors, and Radix builds its shadows from the driven alpha scales.
  const radixColors = new Set(
    "gray mauve slate sage olive sand tomato red ruby crimson pink plum purple violet iris indigo blue cyan teal jade green grass bronze gold brown orange amber yellow lime mint sky black white accent focus".split(
      " ",
    ),
  );
  const scaleVar = /var\(--([a-z]+)-a?\d+\b/g;
  const colorProp = /\bcolor="([a-z]+)"/g;
  const found = [];
  for (const path of webSources) {
    const text = read(path);
    for (const [, scale] of text.matchAll(scaleVar))
      if (radixColors.has(scale) && !driven.has(scale)) found.push(`${path}: --${scale}-*`);
    for (const [, scale] of text.matchAll(colorProp))
      if (!driven.has(scale)) found.push(`${path}: color="${scale}"`);
  }
  assert.deepEqual(found, [], "map the scale in docs/palette.json radix.scales first");
  const app = read("web/src/app/App.tsx");
  assert.match(app, /accentColor="indigo"/);
  assert.match(app, /grayColor="slate"/);
});

test("the page theme color is a swatch", () => {
  const html = read("web/index.html");
  const [, color] = html.match(/name="theme-color" content="(#[0-9a-f]{6})"/);
  assert.ok(hexes.has(color), `${color} is not in the palette`);
  assert.deepEqual(offences("web/index.html", html.replace(color, ""), /#[0-9a-fA-F]{3,8}\b/), []);
});

test("default light and backdrop colors are swatches", () => {
  const lighting = JSON.parse(read("assets/lighting.json"));
  const channels = swatches(spec).map(({ id, hex }) => ({
    id,
    rgb: [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16) / 255),
  }));
  for (const prefix of ["background", "key", "fill"]) {
    const value = ["R", "G", "B"].map((c) => lighting[`${prefix}${c}`]);
    // The lighting panel rounds to two decimals.
    const match = channels.find(({ rgb }) => rgb.every((c, i) => Math.abs(c - value[i]) <= 0.005));
    assert.ok(match, `lighting.json ${prefix} ${value} is not a palette swatch`);
  }
});

test('the color-literal check reads a # after a letter as text, not a color', () => {
  const literal = /(?<![\w])#[0-9a-fA-F]{3,8}\b/;
  assert.equal(literal.test('placeholder="Name#0000"'), false);
  assert.equal(literal.test('color: #0000;'), true);
  assert.equal(literal.test('fill="#abc"'), true);
});
