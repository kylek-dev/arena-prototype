// The slash-command registry (planning/issues/chat.md): every command the
// chat box knows, what it does, and its help line (`domain/commands.ts`
// builds `/help` from this list). Panels open as their menu buttons do;
// admin commands call the door's admin routes, which check the role again.

import {
  findCommand,
  helpLine,
  helpText,
  parseSlash,
  unknownCommandText,
  type CommandInfo,
} from "../domain/commands";
import { isHandle } from "../domain/handles";
import { isQueueMode, queueBlocked, type QueueMode } from "../domain/matchmaking";
import type { Queue } from "./matchmaking";
import { debugEnabled, diagnosticsVisible } from "./diagnostics";
import { characterKits, customizerVisible } from "../state/appearance";
import { accountService } from "../state/account";
import { lightingVisible } from "../state/lighting";
import { mapEditorVisible } from "../state/map-editor";
import { networkLink } from "../state/network";
import { reopenPostMatchScoreboard } from "../state/scoreboard";
import { customLobbyVisible, engine, selectedLayout, setMode, setPanel } from "../state/session";

/** What a command can reach. Tests pass their own. */
export type CommandContext = {
  queue: Queue;
  admin: boolean;
  /** In a network match (seated or watching). */
  inMatch: boolean;
  spectating: boolean;
  /** A system line in the chat box. */
  say(text: string): void;
  fetch(url: string, init?: RequestInit): Promise<Pick<Response, "ok" | "status" | "json">>;
};
export type ChatCommand = CommandInfo & { run(args: string, ctx: CommandContext): void };

const handleArg = (args: string) => {
  const [first = "", ...rest] = args.split(/\s+/);
  return { handle: isHandle(first) ? first : null, rest: rest.join(" ") };
};

/** Calls an admin route; the door answers 401/403 to anyone but an admin. */
async function admin(ctx: CommandContext, path: string, body: object, done: string) {
  try {
    const answer = await ctx.fetch(`/api/admin/${path}`, {
      method: "POST",
      credentials: "same-origin",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (answer.ok) {
      ctx.say(done);
      return;
    }
    // The door's own `{"error"}` when it sent one; a plain-text 403 is an
    // origin it does not allow, and a bare 404 means no admin tools here.
    let code: unknown = null;
    try {
      code = ((await answer.json()) as { error?: unknown } | null)?.error ?? null;
    } catch {
      code = null;
    }
    if (code === "no_such_player") ctx.say("No player has that name.");
    else if (code === "not_admin" || code === "not_logged_in")
      ctx.say("The server refused: that command is for admins.");
    else if (answer.status === 403)
      ctx.say(
        "This page is not allowed to use admin commands. Open the game from its own address.",
      );
    else if (answer.status === 404) ctx.say("This server has no admin tools (accounts are off).");
    else ctx.say(`The server could not do that (HTTP ${answer.status}).`);
  } catch {
    ctx.say("Could not reach the server.");
  }
}

const outsideMatch = (ctx: CommandContext, what: string, open: () => void) => {
  if (ctx.inMatch) ctx.say(`${what} is available outside a match.`);
  else open();
};

export const CHAT_COMMANDS: readonly ChatCommand[] = [
  {
    name: "help",
    usage: "[command]",
    description: "List commands, or explain one.",
    run(args, ctx) {
      if (args) {
        const command = findCommand(CHAT_COMMANDS, args.replace(/^\//, ""), ctx.admin);
        ctx.say(command ? helpLine(command) : unknownCommandText(args.replace(/^\//, "")));
        return;
      }
      for (const line of helpText(CHAT_COMMANDS, ctx.admin)) ctx.say(line);
    },
  },
  {
    name: "menu",
    description: "Open the game menu.",
    run: () => setPanel("menu"),
  },
  {
    name: "settings",
    aliases: ["options"],
    description: "Open settings.",
    run: () => setPanel("settings"),
  },
  {
    name: "keybinds",
    aliases: ["binds"],
    description: "Open settings to change keybinds.",
    run: () => setPanel("settings"),
  },
  {
    name: "spellbook",
    aliases: ["spells"],
    description: "Open the spellbook.",
    run(_, ctx) {
      if (ctx.spectating) ctx.say("Spectators have no spellbook.");
      else setPanel("spellbook");
    },
  },
  {
    name: "customize",
    description: "Customize your character.",
    run(_, ctx) {
      if (!characterKits.get().length) ctx.say("No character kits are loaded.");
      else
        outsideMatch(ctx, "Customizing", () => {
          setPanel(null);
          customizerVisible.set(true);
        });
    },
  },
  {
    name: "layout",
    description: "Move and scale frames, bars and this chat box.",
    run(_, ctx) {
      if (ctx.spectating) ctx.say("Layout editing is not available while spectating.");
      else {
        selectedLayout.set(null);
        setMode("layout");
      }
    },
  },
  {
    name: "quickbind",
    description: "Bind keys by hovering action slots.",
    run(_, ctx) {
      if (ctx.spectating) ctx.say("Quick binding is not available while spectating.");
      else setMode("quick");
    },
  },
  {
    name: "queue",
    usage: "[1v1|2v2|3v3]",
    description: "Search for a match, or open Play online.",
    run(args, ctx) {
      const mode = args.toLowerCase();
      if (!mode) outsideMatch(ctx, "Play online", () => setPanel("menu"));
      else if (!isQueueMode(mode)) ctx.say("Queue for 1v1, 2v2 or 3v3, e.g. /queue 2v2.");
      else if (ctx.inMatch) ctx.say("Finish or leave this match before queueing.");
      else {
        // The same checks as Play online's queue buttons.
        const state = ctx.queue.state.get();
        const blocked = queueBlocked(state, mode as QueueMode);
        if (blocked) ctx.say(blocked);
        else if (state.phase !== "idle") ctx.say("You are already searching for a match.");
        else ctx.queue.queue(mode as QueueMode);
      }
    },
  },
  {
    name: "lobby",
    description: "Open your custom lobby.",
    run(_, ctx) {
      if (!ctx.queue.state.get().lobby)
        ctx.say("You are not in a custom lobby. Host or join one in Play online (/queue).");
      else
        outsideMatch(ctx, "The custom lobby", () => {
          setPanel(null);
          customizerVisible.set(false);
          customLobbyVisible.set(true);
        });
    },
  },
  {
    name: "friends",
    description: "Open Play online, where your friends are.",
    run: (_, ctx) => outsideMatch(ctx, "The friends list", () => setPanel("menu")),
  },
  {
    name: "scoreboard",
    description: "Show the last match's scoreboard again.",
    run(_, ctx) {
      if (!reopenPostMatchScoreboard()) ctx.say("No finished match to show yet.");
    },
  },
  {
    name: "diagnostics",
    description: "Show or hide the performance panel.",
    run(_, ctx) {
      if (!debugEnabled) ctx.say("Diagnostics need ?debug in the page address.");
      else diagnosticsVisible.set(!diagnosticsVisible.get());
    },
  },
  {
    name: "lighting",
    description: "Open the lighting tweaks.",
    run() {
      setPanel(null);
      lightingVisible.set(true);
    },
  },
  {
    name: "mapeditor",
    description: "Open the map editor (admins can also keep maps on the server).",
    run() {
      setPanel(null);
      mapEditorVisible.set(true);
    },
  },
  {
    name: "whisper",
    aliases: ["w"],
    usage: "<Name#number> <text>",
    description: "Whisper to a friend.",
    run(args, ctx) {
      const { handle, rest } = handleArg(args);
      if (!handle || !rest) ctx.say("Whisper with /w Name#number and your message.");
      else ctx.queue.whisper(handle, rest);
    },
  },
  {
    name: "reply",
    aliases: ["r"],
    usage: "<text>",
    description: "Answer whoever whispered last.",
    run(args, ctx) {
      const to = ctx.queue.chat.get().whisperTo;
      if (!to || !isHandle(to)) ctx.say("Nobody to answer yet.");
      else if (!args) ctx.say("Type your answer after /r.");
      else ctx.queue.whisper(to, args);
    },
  },
  {
    name: "ignore",
    usage: "<Name#number>",
    description: "Stop hearing a player.",
    run(args, ctx) {
      const { handle } = handleArg(args);
      if (!handle) ctx.say("Ignore with /ignore Name#number.");
      else ctx.queue.ignore(handle, true);
    },
  },
  {
    name: "unignore",
    usage: "<Name#number>",
    description: "Hear a player again.",
    run(args, ctx) {
      const { handle } = handleArg(args);
      if (!handle) ctx.say("Unignore with /unignore Name#number.");
      else ctx.queue.ignore(handle, false);
    },
  },
  {
    name: "clear",
    description: "Empty the chat box.",
    run: (_, ctx) => ctx.queue.clearChat(),
  },
  {
    name: "mute",
    usage: "<Name#number> [minutes]",
    description: "Stop a player's chat (up to a week; 60 minutes by default).",
    admin: true,
    run(args, ctx) {
      const { handle, rest } = handleArg(args);
      const minutes = rest ? Number(rest) : 60;
      if (!handle || !Number.isInteger(minutes) || minutes < 1 || minutes > 10_080)
        ctx.say("Mute with /mute Name#number and 1–10080 minutes.");
      else void admin(ctx, "mute", { handle, minutes }, `Muted ${handle} for ${minutes} min.`);
    },
  },
  {
    name: "unmute",
    usage: "<Name#number>",
    description: "Let a muted player chat again.",
    admin: true,
    run(args, ctx) {
      const { handle } = handleArg(args);
      if (!handle) ctx.say("Unmute with /unmute Name#number.");
      else void admin(ctx, "mute", { handle, minutes: 0 }, `Unmuted ${handle}.`);
    },
  },
  {
    name: "clearname",
    usage: "<Name#number>",
    description: "Remove a player's name; they pick a new one.",
    admin: true,
    run(args, ctx) {
      const { handle } = handleArg(args);
      if (!handle) ctx.say("Clear a name with /clearname Name#number.");
      else void admin(ctx, "handles/clear", { handle }, `Cleared ${handle}'s name.`);
    },
  },
];

/** Runs `text` if it is a command; `false` when it is chat to send. */
export function runSlash(text: string, ctx: CommandContext): boolean {
  const typed = parseSlash(text);
  if (!typed) return false;
  // A bare `/` is nothing to run (and nothing to send).
  if (!typed.name) {
    if (typed.args) ctx.say("Type the command right after the slash, e.g. /help.");
    return true;
  }
  const command = findCommand(CHAT_COMMANDS, typed.name, ctx.admin);
  if (command) command.run(typed.args, ctx);
  else ctx.say(unknownCommandText(typed.name));
  return true;
}

/** The page's context: the live queue, account and match. */
export function pageCommandContext(queue: Queue): CommandContext {
  // As the game menu decides them.
  const link = networkLink.get()?.state.get();
  const phase = link?.phase ?? "idle";
  const snapshot = engine.get();
  return {
    queue,
    admin: !!accountService.get()?.state.get().me?.admin,
    inMatch: phase !== "idle" && phase !== "ended",
    spectating: !!link?.status?.spectator || !!snapshot?.spectating,
    say: (text) => queue.system(text),
    fetch: (url, init) => fetch(new URL(url, window.location.origin), init),
  };
}
