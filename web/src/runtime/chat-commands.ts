// The slash-command registry (planning/issues/chat.md): every command the
// chat box knows, what it does, and its help line (`domain/commands.ts`
// builds `/help` from this list). Panels open as their launcher and menu buttons do;
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
import { mapEditorVisible } from "../state/map-editor";
import { networkLink } from "../state/network";
import { friendsWindowVisible, openPlay } from "../state/windows";
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
      ctx.say("This command is available to administrators only.");
    else if (answer.status === 403)
      ctx.say(
        "This page is not allowed to use admin commands. Open the game from its own address.",
      );
    else if (answer.status === 404) ctx.say("This server does not offer administrator tools.");
    else ctx.say(`The server could not do that (HTTP ${answer.status}).`);
  } catch {
    ctx.say("The server could not be reached.");
  }
}

const outsideMatch = (ctx: CommandContext, what: string, open: () => void) => {
  if (ctx.inMatch) ctx.say(`${what} is unavailable during a match.`);
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
      if (ctx.spectating) ctx.say("The spellbook is unavailable while spectating.");
      else setPanel("spellbook");
    },
  },
  {
    name: "customize",
    description: "Customize your character.",
    run(_, ctx) {
      if (!characterKits.get().length)
        ctx.say("Customization is unavailable because no character is loaded.");
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
      if (ctx.spectating) ctx.say("Layout editing is unavailable while spectating.");
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
      if (ctx.spectating) ctx.say("Quick binding is unavailable while spectating.");
      else setMode("quick");
    },
  },
  {
    name: "queue",
    aliases: ["play"],
    usage: "[1v1|2v2|3v3]",
    description: "Join a public queue, or open the Play window on its Queue tab.",
    run(args, ctx) {
      const mode = args.toLowerCase();
      if (!mode) openPlay("queue");
      else if (!isQueueMode(mode))
        ctx.say("Specify a queue: /queue 1v1, /queue 2v2 or /queue 3v3.");
      else if (ctx.inMatch) ctx.say("Leave the current match before joining a queue.");
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
    name: "practice",
    description: "Open the Play window on its Practice tab.",
    run: () => openPlay("practice"),
  },
  {
    name: "host",
    description: "Open the Play window on its Host tab.",
    run: () => openPlay("host"),
  },
  {
    name: "lobby",
    description: "Open your custom lobby.",
    run(_, ctx) {
      if (!ctx.queue.state.get().lobby)
        ctx.say("You are not in a custom lobby. Host or join one from the Play window (/host).");
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
    description: "Open the Friends window.",
    run: () => friendsWindowVisible.set(true),
  },
  {
    name: "scoreboard",
    description: "Show the last match's scoreboard again.",
    run(_, ctx) {
      if (!reopenPostMatchScoreboard()) ctx.say("No completed match is available.");
    },
  },
  {
    name: "diagnostics",
    description: "Show or hide the performance panel.",
    run(_, ctx) {
      if (!debugEnabled) ctx.say("Diagnostics require ?debug in the page address.");
      else diagnosticsVisible.set(!diagnosticsVisible.get());
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
    usage: "<Name#0000> <message>",
    description: "Whisper to a friend.",
    run(args, ctx) {
      const { handle, rest } = handleArg(args);
      if (!handle || !rest) ctx.say("Usage: /w Name#0000 followed by your message.");
      else ctx.queue.whisper(handle, rest);
    },
  },
  {
    name: "reply",
    aliases: ["r"],
    usage: "<message>",
    description: "Answer the last conversation.",
    run(args, ctx) {
      const to = ctx.queue.chat.get().whisperTo;
      if (!to || !isHandle(to)) ctx.say("No one has whispered to you yet.");
      else if (!args) ctx.say("Usage: /r followed by your reply.");
      else ctx.queue.whisper(to, args);
    },
  },
  {
    name: "ignore",
    usage: "<Name#0000>",
    description: "Stop hearing a player.",
    run(args, ctx) {
      const { handle } = handleArg(args);
      if (!handle) ctx.say("Usage: /ignore Name#0000.");
      else ctx.queue.ignore(handle, true);
    },
  },
  {
    name: "unignore",
    usage: "<Name#0000>",
    description: "Hear a player again.",
    run(args, ctx) {
      const { handle } = handleArg(args);
      if (!handle) ctx.say("Usage: /unignore Name#0000.");
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
    usage: "<Name#0000> [minutes]",
    description: "Stop a player's chat (up to a week; 60 minutes by default).",
    admin: true,
    run(args, ctx) {
      const { handle, rest } = handleArg(args);
      const minutes = rest ? Number(rest) : 60;
      if (!handle || !Number.isInteger(minutes) || minutes < 1 || minutes > 10_080)
        ctx.say("Usage: /mute Name#0000 followed by 1 to 10080 minutes.");
      else
        void admin(ctx, "mute", { handle, minutes }, `${handle} is muted for ${minutes} minutes.`);
    },
  },
  {
    name: "unmute",
    usage: "<Name#0000>",
    description: "Let a muted player chat again.",
    admin: true,
    run(args, ctx) {
      const { handle } = handleArg(args);
      if (!handle) ctx.say("Usage: /unmute Name#0000.");
      else void admin(ctx, "mute", { handle, minutes: 0 }, `${handle} is no longer muted.`);
    },
  },
  {
    name: "clearname",
    usage: "<Name#0000>",
    description: "Remove a player's name; they pick a new one.",
    admin: true,
    run(args, ctx) {
      const { handle } = handleArg(args);
      if (!handle) ctx.say("Usage: /clearname Name#0000.");
      else void admin(ctx, "handles/clear", { handle }, `The name ${handle} has been cleared.`);
    },
  },
];

/** Runs `text` if it is a command; `false` when it is chat to send. */
export function runSlash(text: string, ctx: CommandContext): boolean {
  const typed = parseSlash(text);
  if (!typed) return false;
  // A bare `/` is nothing to run (and nothing to send).
  if (!typed.name) {
    if (typed.args) ctx.say("Type the command directly after the slash, for example /help.");
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
