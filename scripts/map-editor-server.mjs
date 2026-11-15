// Local map-editor endpoints for the Vite dev and preview servers only; the
// static production build never contains them.
//
//   GET /assets/maps/<id>.json        the tracked file, fresh from assets/maps
//   GET /__map-editor/status          {"writable": true}
//   GET /__map-editor/maps            {"maps": [{"id", "name"}]} from assets/maps
//   PUT /__map-editor/maps/<id>.json  writes assets/maps/<id>.json
//
// Writes require a loopback Host (blocks DNS rebinding), a same-origin Origin
// when the browser sends one, and a JSON content type (cross-site pages cannot
// send that without a preflight this server never approves). The body must be
// a format-1 map object; Rust validates the gameplay rules before the editor
// enables saving, and the build and tests validate the tracked file again.
import { readdir, readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ID = /^[a-z0-9][a-z0-9-]{0,40}$/;
const MAX_BYTES = 256 * 1024;
const LOOPBACK = /^(127\.0\.0\.1|localhost|\[::1\])(:\d+)?$/;

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function send(res, status, body, type = "application/json") {
  res.statusCode = status;
  res.setHeader("Content-Type", type);
  res.setHeader("Cache-Control", "no-store");
  res.end(typeof body === "string" ? body : JSON.stringify(body));
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    // Oversized bodies are drained, not kept, so the client still gets a 413.
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size <= MAX_BYTES) chunks.push(chunk);
    });
    req.on("end", () =>
      size > MAX_BYTES
        ? reject(new Error("too large"))
        : resolve(Buffer.concat(chunks).toString("utf8")),
    );
    req.on("error", reject);
  });
}

/** Every map file's id and display name, for the editor's Open list. */
export async function listMapFiles(mapsDir) {
  const maps = [];
  for (const file of (await readdir(mapsDir).catch(() => [])).sort()) {
    const id = file.endsWith(".json") ? file.slice(0, -5) : "";
    if (!ID.test(id)) continue;
    let name = id;
    try {
      const parsed = JSON.parse(await readFile(path.join(mapsDir, file), "utf8"));
      if (typeof parsed?.name === "string" && parsed.name.trim()) name = parsed.name;
    } catch {
      // Listed by id; opening it reports the problem.
    }
    maps.push({ id, name });
  }
  return maps;
}

/** Connect-style middleware; `root` is the repository (tests pass a temp dir). */
export function mapEditorMiddleware(root = repositoryRoot) {
  const mapsDir = path.join(root, "assets", "maps");
  return async (req, res, next) => {
    const pathname = (req.url ?? "").split("?")[0];
    const read = pathname.match(/^\/assets\/maps\/([^/]+)\.json$/);
    const write = pathname.match(/^\/__map-editor\/maps\/([^/]+)\.json$/);
    if (pathname === "/__map-editor/status" && req.method === "GET")
      return send(res, 200, { writable: true });
    if (pathname === "/__map-editor/maps" && req.method === "GET")
      return send(res, 200, { maps: await listMapFiles(mapsDir) });
    if (read && req.method === "GET" && ID.test(read[1])) {
      try {
        return send(res, 200, await readFile(path.join(mapsDir, `${read[1]}.json`), "utf8"));
      } catch {
        return next();
      }
    }
    if (!write) return next();
    if (req.method !== "PUT") return send(res, 405, { error: "Use PUT to save a map." });
    if (!ID.test(write[1]))
      return send(res, 400, { error: "Map ids use lowercase letters, digits and dashes." });
    const host = req.headers.host ?? "";
    if (!LOOPBACK.test(host))
      return send(res, 403, { error: "Maps can only be saved from a loopback address." });
    const origin = req.headers.origin;
    if (origin !== undefined && origin !== `http://${host}`)
      return send(res, 403, { error: "Cross-origin saves are refused." });
    if (!String(req.headers["content-type"] ?? "").startsWith("application/json"))
      return send(res, 415, { error: "Send the map as application/json." });
    let text;
    try {
      text = await readBody(req);
    } catch {
      return send(res, 413, { error: "Map files are limited to 256 KiB." });
    }
    let map;
    try {
      map = JSON.parse(text);
    } catch {
      return send(res, 400, { error: "The map is not valid JSON." });
    }
    if (typeof map !== "object" || map === null || Array.isArray(map) || map.format !== 1)
      return send(res, 400, { error: "Only format-1 map files can be saved." });
    const target = path.join(mapsDir, `${write[1]}.json`);
    const temporary = `${target}.${process.pid}.tmp`;
    await writeFile(temporary, text.endsWith("\n") ? text : `${text}\n`);
    await rename(temporary, target);
    return send(res, 200, { saved: `assets/maps/${write[1]}.json` });
  };
}

/** Vite plugin adding the middleware to `vite` and `vite preview`. */
export function mapEditorServer(root = repositoryRoot) {
  return {
    name: "arena-map-editor",
    configureServer(server) {
      server.middlewares.use(mapEditorMiddleware(root));
    },
    configurePreviewServer(server) {
      server.middlewares.use(mapEditorMiddleware(root));
    },
  };
}
