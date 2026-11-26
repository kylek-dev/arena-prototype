// Slash commands typed in the chat box (planning/issues/chat.md): parsing,
// lookup and the help text, all from one registry so `/help` cannot fall
// out of date. What each command does lives with the registry in
// `runtime/chat-commands.ts`. Pure, so it is unit tested without a page.

export type CommandInfo = {
  /** Typed after the slash, lowercase. */
  name: string;
  aliases?: readonly string[];
  /** What follows the name, e.g. `<Name#number> <text>`. */
  usage?: string;
  description: string;
  /** Listed and offered only to admins; the door checks it again. */
  admin?: boolean;
};

/** A typed command: the name (lowercase, empty for a bare `/`) and the rest
 * of the line. `null` when the text is not a command; `//text` is plain text
 * starting with `/`. */
export function parseSlash(text: string): { name: string; args: string } | null {
  const line = text.trim();
  if (!line.startsWith("/") || line.startsWith("//")) return null;
  const match = /^\/(\S*)\s*([\s\S]*)$/.exec(line);
  return { name: (match?.[1] ?? "").toLowerCase(), args: (match?.[2] ?? "").trim() };
}

/** Text typed with a leading `//`, sent as chat beginning with one `/`. */
export function unescapeSlash(text: string) {
  const line = text.trim();
  return line.startsWith("//") ? line.slice(1) : line;
}

/** The command a typed name means, for this player; admin commands are
 * unknown to everyone else. */
export function findCommand<C extends CommandInfo>(
  registry: readonly C[],
  name: string,
  admin: boolean,
): C | null {
  const wanted = name.toLowerCase();
  return (
    registry.find(
      (command) =>
        (admin || !command.admin) && (command.name === wanted || command.aliases?.includes(wanted)),
    ) ?? null
  );
}

/** One help line: `/whisper <Name#number> <text> (also /w) — Whisper to a friend.` */
export function helpLine(command: CommandInfo) {
  const usage = command.usage ? ` ${command.usage}` : "";
  const aliases = command.aliases?.length
    ? ` (also ${command.aliases.map((alias) => `/${alias}`).join(", ")})`
    : "";
  return `/${command.name}${usage}${aliases} — ${command.description}`;
}

/** `/help`: every command this player may use, from the registry. */
export function helpText(registry: readonly CommandInfo[], admin: boolean): string[] {
  const shown = registry.filter((command) => admin || !command.admin);
  return [
    "Commands (type // to start a message with /):",
    ...shown.filter((command) => !command.admin).map(helpLine),
    ...(admin ? ["Admin commands:", ...shown.filter((c) => c.admin).map(helpLine)] : []),
  ];
}

export function unknownCommandText(name: string) {
  return `Unknown command /${name}. Type /help for the list.`;
}
