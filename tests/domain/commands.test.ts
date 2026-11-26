import { expect, it } from "vitest";
import {
  findCommand,
  helpLine,
  helpText,
  parseSlash,
  unescapeSlash,
  unknownCommandText,
} from "../../web/src/domain/commands";
import { CHAT_COMMANDS } from "../../web/src/runtime/chat-commands";

it("parses commands, leaving chat and escaped slashes alone", () => {
  expect(parseSlash("/help")).toEqual({ name: "help", args: "" });
  expect(parseSlash("  /W  Rook#0042   hi there ")).toEqual({
    name: "w",
    args: "Rook#0042   hi there",
  });
  expect(parseSlash("hello /help")).toBeNull();
  expect(parseSlash("//shrug")).toBeNull();
  // A bare slash, or one followed by a space, is a command with no name.
  expect(parseSlash("/")).toEqual({ name: "", args: "" });
  expect(parseSlash("/ help")).toEqual({ name: "", args: "help" });
  expect(unescapeSlash("//shrug")).toBe("/shrug");
  expect(unescapeSlash("plain")).toBe("plain");
});

it("finds commands by name or alias, and hides admin commands from others", () => {
  expect(findCommand(CHAT_COMMANDS, "w", false)?.name).toBe("whisper");
  expect(findCommand(CHAT_COMMANDS, "SPELLS", false)?.name).toBe("spellbook");
  expect(findCommand(CHAT_COMMANDS, "mute", false)).toBeNull();
  expect(findCommand(CHAT_COMMANDS, "mute", true)?.admin).toBe(true);
  expect(findCommand(CHAT_COMMANDS, "dance", true)).toBeNull();
  expect(unknownCommandText("dance")).toBe("Unknown command /dance. Type /help for the list.");
});

it("builds help from the registry, so every command is listed exactly once", () => {
  const player = helpText(CHAT_COMMANDS, false);
  const admin = helpText(CHAT_COMMANDS, true);
  const listed = (lines: string[]) =>
    lines.filter((line) => line.startsWith("/")).map((line) => line.split(" ")[0]);
  expect(listed(player)).toEqual(CHAT_COMMANDS.filter((c) => !c.admin).map((c) => `/${c.name}`));
  expect(listed(admin)).toHaveLength(CHAT_COMMANDS.length);
  expect(player.some((line) => line.startsWith("/mute"))).toBe(false);
  expect(admin).toContain("Admin commands:");
  expect(helpLine(findCommand(CHAT_COMMANDS, "w", false)!)).toBe(
    "/whisper <Name#0000> <message> (also /w) — Whisper to a friend.",
  );
  // Names are unique, aliases never shadow a name.
  const names = CHAT_COMMANDS.flatMap((c) => [c.name, ...(c.aliases ?? [])]);
  expect(new Set(names).size).toBe(names.length);
  // Every existing panel has a command.
  for (const panel of [
    "spellbook",
    "keybinds",
    "settings",
    "customize",
    "layout",
    "queue",
    "lobby",
    "scoreboard",
    "diagnostics",
    "mapeditor",
    "menu",
    "friends",
    "quickbind",
  ])
    expect(findCommand(CHAT_COMMANDS, panel, false), panel).not.toBeNull();
});
