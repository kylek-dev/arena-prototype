import { describe, expect, it } from "vitest";
import {
  backoffMs,
  encodeHello,
  ERROR_CODES,
  isFinalError,
  isWelcome,
  PROTOCOL_VERSION,
  serverError,
  parseJoinFragment,
  parseStatus,
  parseTicket,
  type MatchTicket,
} from "../../web/src/domain/network";
import {
  createLink,
  installJoinLinks,
  takeJoinFragment,
  type LinkDeps,
} from "../../web/src/runtime/network";

const NOW = 1_800_000_000_000;
const ticket: MatchTicket = {
  server: "ws://127.0.0.1:8080/play",
  ticket: "ab01",
  expires: NOW / 1000 + 3600,
};
const status = (patch: object) => ({
  phase: "playing",
  error: null,
  seat: 1,
  mapDiffers: false,
  rttMs: 80,
  inputMs: 120,
  delayMs: 100,
  snapshots: 10,
  underruns: 0,
  hashMismatches: 0,
  predictedTicks: 3,
  corrections: 0,
  lastCorrectionMm: null,
  queued: 1.1,
  queuedMin: 1,
  pace: 1.004,
  skippedInputs: 0,
  ...patch,
});

class FakeSocket {
  binaryType = "blob";
  readyState = 0;
  sent: Uint8Array[] = [];
  closed = false;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  constructor(readonly url: string) {}
  send(data: Uint8Array) {
    this.sent.push(data);
  }
  close() {
    this.closed = true;
  }
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
  drop() {
    this.readyState = 3;
    this.onclose?.();
  }
  receive(bytes: number[]) {
    this.onmessage?.({ data: new Uint8Array(bytes).buffer });
  }
}

function memory() {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
    removeItem: (key: string) => void values.delete(key),
  };
}

function harness() {
  const sockets: FakeSocket[] = [];
  const timers: { run: () => void; ms: number; id: number }[] = [];
  let now = NOW;
  const tab = memory();
  const device = memory();
  const deps: LinkDeps = {
    createSocket: (url) => {
      const socket = new FakeSocket(url);
      sockets.push(socket);
      return socket;
    },
    now: () => now,
    setTimer: (run, ms) => {
      const id = timers.length + 1;
      timers.push({ run, ms, id });
      return id;
    },
    clearTimer: (id) => {
      const index = timers.findIndex((timer) => timer.id === id);
      if (index >= 0) timers.splice(index, 1);
    },
    tab,
    device,
  };
  const link = createLink(deps);
  const fire = () => {
    const timer = timers.shift();
    if (!timer) throw new Error("no timer pending");
    now += timer.ms;
    timer.run();
    return timer.ms;
  };
  return {
    link,
    sockets,
    timers,
    tab,
    device,
    fire,
    advance: (ms: number) => (now += ms),
    latest: () => sockets[sockets.length - 1],
  };
}

describe("network match identity", () => {
  it("accepts only websocket servers, hex tickets and real expiry times", () => {
    expect(parseTicket(ticket)).toEqual(ticket);
    expect(parseTicket({ ...ticket, ticket: "AB01" })?.ticket).toBe("ab01");
    for (const bad of [
      null,
      { ...ticket, server: "http://example.test/play" },
      { ...ticket, server: "not a url" },
      { ...ticket, ticket: "abc" },
      { ...ticket, ticket: "zz" },
      { ...ticket, ticket: "" },
      { ...ticket, ticket: "ab".repeat(513) },
      { ...ticket, expires: 1.5 },
      { ...ticket, expires: "soon" },
    ])
      expect(parseTicket(bad)).toBeNull();
  });

  it("reads development join links from the fragment only", () => {
    const hash = `#server=${encodeURIComponent(ticket.server)}&ticket=ab01&expires=${ticket.expires}`;
    expect(parseJoinFragment(hash)).toEqual(ticket);
    expect(parseJoinFragment("")).toBeNull();
    expect(parseJoinFragment("#debug=1")).toBeNull();
    expect(parseJoinFragment("#server=ws%3A%2F%2Fx%2Fplay&ticket=ab01&expires=x")).toBeNull();
  });

  it("backs off from half a second to eight and knows which errors are final", () => {
    expect([0, 1, 2, 3, 4, 5, 9].map(backoffMs)).toEqual([500, 1000, 2000, 4000, 8000, 8000, 8000]);
    for (const error of ["MatchOver", "BadTicket", "VersionMismatch", "MatchFull"])
      expect(isFinalError(error)).toBe(true);
    for (const error of ["ServerShutdown", "RateLimited", "Malformed", "HelloTimeout", null])
      expect(isFinalError(error)).toBe(false);
  });

  it("validates Rust's status reports", () => {
    expect(parseStatus(status({}))).toEqual(status({}));
    expect(
      parseStatus(status({ phase: "closed", error: "MatchOver", rttMs: null })),
    ).not.toBeNull();
    for (const bad of [
      null,
      status({ phase: "lost" }),
      status({ seat: 6 }),
      status({ rttMs: Number.NaN }),
      status({ underruns: -1 }),
      status({ mapDiffers: "no" }),
      status({ predictedTicks: 1.5 }),
      status({ lastCorrectionMm: -1 }),
      status({ pace: 3 }),
      status({ queued: "one" }),
      status({ queuedMin: 0.5 }),
      status({ skippedInputs: -2 }),
    ])
      expect(parseStatus(bad)).toBeNull();
  });
});

describe("protocol corners the runtime speaks itself", () => {
  it("encodes Hello exactly as arena-protocol's golden frame", () => {
    // arena-protocol: Hello { protocol: 1, ticket: [0xab, 0xcd] } == [0, 1, 2, 0xab, 0xcd]
    expect([...encodeHello("abcd")]).toEqual([0, 1, 2, 0xab, 0xcd]);
    expect(PROTOCOL_VERSION).toBe(1);
    // Lengths and versions past 127 use more varint bytes.
    const long = encodeHello("00".repeat(300), 300);
    expect([...long.slice(0, 5)]).toEqual([0, 0xac, 0x02, 0xac, 0x02]);
    expect(long.length).toBe(5 + 300);
  });

  it("reads Error frames with arena-protocol's code order and nothing else", () => {
    expect(ERROR_CODES.map((_, index) => serverError(new Uint8Array([3, index])))).toEqual([
      "VersionMismatch",
      "BadTicket",
      "MatchFull",
      "MatchOver",
      "RateLimited",
      "Malformed",
      "HelloTimeout",
      "ServerShutdown",
      "Replaced",
    ]);
    expect(serverError(new Uint8Array([3, 99]))).toBeNull();
    expect(serverError(new Uint8Array([1, 3]))).toBeNull();
    expect(serverError(new Uint8Array([3, 3, 0]))).toBeNull();
    expect(isWelcome(new Uint8Array([0, 1]))).toBe(true);
    expect(isWelcome(new Uint8Array([1, 0]))).toBe(false);
  });
});

describe("network link", () => {
  it("greets each socket itself and hands the engine its frames by generation", () => {
    const { link, latest } = harness();
    expect(link.read()).toEqual([]);
    link.join(ticket);
    expect(latest().url).toBe(ticket.server);
    expect(latest().binaryType).toBe("arraybuffer");
    // Connecting: generation 0 tells the engine there is no socket yet.
    expect(link.read()).toEqual([0]);
    latest().open();
    // The runtime sends Hello at once, even if the engine is paused.
    expect(latest().sent).toEqual([encodeHello("ab01")]);
    latest().receive([1, 2]);
    latest().receive([3]);
    expect(link.read()).toEqual([1, new Uint8Array([1, 2]), new Uint8Array([3])]);
    expect(link.read()).toEqual([1]);
    link.send(new Uint8Array([9]));
    expect(latest().sent.at(-1)).toEqual(new Uint8Array([9]));
    expect(link.state.get().phase).toBe("open");
  });

  it("reconnects with backoff until a socket opens, then joins as a new generation", () => {
    const { link, latest, fire, sockets } = harness();
    link.join(ticket);
    latest().open();
    latest().receive([1, 7]);
    latest().drop();
    expect(link.state.get()).toMatchObject({ phase: "reconnecting", attempt: 1 });
    // The closed socket's last frames are still delivered under its generation.
    expect(link.read()).toEqual([1, new Uint8Array([1, 7])]);
    // Sends while disconnected are dropped, not queued.
    link.send(new Uint8Array([1]));
    expect(fire()).toBe(500);
    latest().drop();
    expect(fire()).toBe(1000);
    latest().drop();
    expect(fire()).toBe(2000);
    latest().open();
    expect(link.state.get()).toMatchObject({ phase: "open", attempt: 0 });
    expect(link.read()).toEqual([2]);
    expect(sockets.length).toBe(4);
    // Only the opened sockets were greeted; nothing else was sent.
    expect(sockets.map((socket) => socket.sent.length)).toEqual([1, 0, 0, 1]);
  });

  it("stops on a final server error even while the engine is paused", () => {
    const { link, latest, timers, tab, device } = harness();
    link.join(ticket);
    expect(tab.values.size).toBe(1);
    expect(device.values.size).toBe(1);
    latest().open();
    // The server says MatchOver and closes; no engine read happens.
    latest().receive([3, 3]);
    latest().drop();
    expect(timers.length).toBe(0);
    expect(link.state.get()).toMatchObject({ phase: "ended", ended: "MatchOver" });
    expect(link.read()).toEqual([]);
    expect(tab.values.size + device.values.size).toBe(0);
    // A retryable error keeps trying.
    const other = harness();
    other.link.join(ticket);
    other.latest().open();
    other.latest().receive([3, 7]);
    other.latest().drop();
    expect(other.link.state.get().phase).toBe("reconnecting");
    // The engine's report of a final error also ends the link.
    other.link.onStatus(status({ phase: "closed", error: "BadTicket" }));
    expect(other.link.state.get()).toMatchObject({ phase: "ended", ended: "BadTicket" });
    // Malformed reports are ignored.
    const third = harness();
    third.link.join(ticket);
    third.link.onStatus({ phase: "closed", error: 5 });
    expect(third.link.state.get().phase).toBe("connecting");
  });

  it("gives a seat up to a newer tab without forgetting it, so it can be taken back", () => {
    const { link, latest, timers, device } = harness();
    link.join(ticket);
    latest().open();
    latest().receive([3, 8]);
    latest().drop();
    expect(timers.length).toBe(0);
    expect(link.state.get()).toMatchObject({ phase: "ended", ended: "Replaced" });
    expect(link.resumable()).toBeNull();
    expect(device.values.size).toBe(1);
    expect(link.remembered()).toEqual(ticket);
  });

  it("keeps the Welcome when a background tab's backlog overflows", () => {
    const { link, latest } = harness();
    link.join(ticket);
    latest().open();
    latest().receive([0, 1, 1]);
    for (let index = 0; index < 600; index += 1) latest().receive([1, index & 0xff]);
    const [generation, ...frames] = link.read() as [number, ...Uint8Array[]];
    expect(generation).toBe(1);
    expect(frames).toHaveLength(512);
    expect([...frames[0]]).toEqual([0, 1, 1]);
    expect([...frames[1]]).toEqual([1, 89]);
    expect([...frames[511]]).toEqual([1, 599 & 0xff]);
  });

  it("stops retrying when the ticket expires", () => {
    const { link, latest, fire, advance } = harness();
    link.join(ticket);
    latest().drop();
    advance(3600 * 1000);
    fire();
    expect(link.state.get()).toMatchObject({ phase: "ended", ended: "expired" });
  });

  it("joins a link pasted into an open tab and removes it from the address bar", () => {
    const { link, latest } = harness();
    const stop = installJoinLinks(link);
    window.location.hash = `server=${encodeURIComponent(ticket.server)}&ticket=ab01&expires=${ticket.expires}`;
    window.dispatchEvent(new HashChangeEvent("hashchange"));
    expect(link.state.get().phase).toBe("connecting");
    expect(latest().url).toBe(ticket.server);
    expect(window.location.hash).toBe("");
    stop();
    window.location.hash = `server=${encodeURIComponent(ticket.server)}&ticket=cd02&expires=${ticket.expires}`;
    window.dispatchEvent(new HashChangeEvent("hashchange"));
    expect(link.resumable()?.ticket).toBe("ab01");
    expect(takeJoinFragment()?.ticket).toBe("cd02");
    expect(window.location.hash).toBe("");
  });

  it("remembers the match per tab for reloads and per device for an offered rejoin", () => {
    const first = harness();
    first.link.join(ticket);
    const reloaded = harness();
    for (const [key, value] of first.tab.values) reloaded.tab.setItem(key, value);
    for (const [key, value] of first.device.values) reloaded.device.setItem(key, value);
    expect(reloaded.link.resumable()).toEqual(ticket);
    expect(reloaded.link.remembered()).toEqual(ticket);
    // Leaving on purpose forgets this tab's seat but still offers a rejoin.
    first.link.leave();
    expect(first.link.resumable()).toBeNull();
    expect(first.link.remembered()).toEqual(ticket);
    expect(first.link.state.get().phase).toBe("idle");
    // Corrupt or expired storage is ignored.
    const corrupt = harness();
    corrupt.tab.setItem("arena.match", "{not json");
    corrupt.device.setItem("arena.lastMatch", JSON.stringify({ ...ticket, expires: 1 }));
    expect(corrupt.link.resumable()).toBeNull();
    expect(corrupt.link.remembered()).toBeNull();
    // Teardown closes the socket but keeps the tab's seat for the reload.
    const torn = harness();
    torn.link.join(ticket);
    torn.link.dispose();
    expect(torn.latest().closed).toBe(true);
    expect(torn.link.resumable()).toEqual(ticket);
    expect(torn.link.read()).toEqual([]);
  });
});
