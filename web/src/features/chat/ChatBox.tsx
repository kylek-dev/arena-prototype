import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import {
  MAX_CHAT_CHARS,
  chatErrorText,
  chatPrefix,
  chatTabLabel,
  chatTabs,
  chatText,
  isIgnored,
  tabMessages,
  type ChatChannel,
  type ChatMessage,
} from "../../domain/chat";
import { isHandle } from "../../domain/handles";
import type { Queue } from "../../runtime/matchmaking";
import { chatFocusRequest, chatShown } from "../../state/chat";
import { updateConfig, useConfigSelector } from "../../state/config";
import { anchorAt } from "../../domain/layout/model";
import { clampChatSize, type ChatSize } from "../../domain/preferences/schema";
import { matchQueue } from "../../state/network";
import { engine, interaction } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { accountService } from "../../state/account";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { pageCommandContext, runSlash } from "../../runtime/chat-commands";
import { unescapeSlash } from "../../domain/commands";
import { AdminTag } from "../../ui/AdminTag";
import { Placed } from "../../ui/Placed";
import "./chat.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);
/** A resize in progress: the size, and where the top-left corner stays. */
type ChatDraft = { size: ChatSize; left: number; top: number; scale: number };
/** Lines shown per tab; older ones stay in the log. */
const SHOWN = 100;

/** Whisper or (un)ignore a player from their name in the log. */
function NameMenu({
  queue,
  handle,
  ignored,
  close,
}: {
  queue: Queue;
  handle: string;
  ignored: boolean;
  close: () => void;
}) {
  return (
    <span className="chat-name-menu" role="menu" aria-label={`${handle} actions`}>
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          queue.whisperTo(handle);
          queue.showChat("whisper");
          close();
        }}
      >
        Whisper
      </button>
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          queue.ignore(handle, !ignored);
          close();
        }}
      >
        {ignored ? "Unignore" : "Ignore"}
      </button>
    </span>
  );
}

function Line({
  queue,
  message,
  ignored,
  menu,
  setMenu,
  me,
}: {
  queue: Queue;
  message: ChatMessage;
  ignored: (handle: string) => boolean;
  menu: boolean;
  setMenu: (id: number | null) => void;
  /** This player's handle, if known: their own lines get no menu. */
  me: string | null;
}) {
  // The page's own lines: styled apart, and nobody to whisper or ignore.
  if (message.channel === "system") return <li className="chat-line system">{message.text}</li>;
  // One's own whisper names the recipient; other lines name the sender.
  const who = message.to ?? message.from;
  const own = !!me && message.to === null && message.from.toLowerCase() === me.toLowerCase();
  return (
    <li className={`chat-line ${message.channel}`}>
      {own ? (
        <span className="chat-name">{chatPrefix(message)}</span>
      ) : (
        <button
          type="button"
          className="chat-name"
          aria-expanded={menu}
          aria-label={`${chatPrefix(message)}: options`}
          onClick={() => setMenu(menu ? null : message.id)}
        >
          {chatPrefix(message)}
        </button>
      )}
      {message.admin && message.to === null && <AdminTag compact />}
      <span className="chat-text">: {message.text}</span>
      {menu && (
        <NameMenu queue={queue} handle={who} ignored={ignored(who)} close={() => setMenu(null)} />
      )}
    </li>
  );
}

/** The bottom-right grip: drag (or arrow keys) to change the box's width
 * and height, down to its minimum size; its top-left corner stays put. */
function ResizeGrip({
  size,
  onDraft,
  onCommit,
}: {
  size: ChatSize;
  onDraft: (draft: ChatDraft | null) => void;
  onCommit: (draft: ChatDraft) => void;
}) {
  const start = useRef<{
    x: number;
    y: number;
    size: ChatSize;
    rect: DOMRect;
    scale: number;
  } | null>(null);
  const latest = useRef<ChatDraft | null>(null);
  /** The new size, and where the top-left corner has to stay. */
  const draftFor = (element: HTMLElement, rect: DOMRect, next: ChatSize, scale: number) => {
    const placed = element.closest(".placed");
    // Never bigger than the screen holds at this scale, or the frame's
    // effective scale would drop mid-drag and the grip slip off the pointer.
    const fits = clampChatSize({
      width: Math.min(next.width, (innerWidth - 24) / scale),
      height: Math.min(next.height, (innerHeight - 24) / scale),
    });
    return {
      size: fits,
      left: (placed?.getBoundingClientRect().left ?? rect.left) - 12,
      top: (placed?.getBoundingClientRect().top ?? rect.top) - 12,
      scale,
    };
  };
  const box = (element: HTMLElement) => {
    const section = element.closest(".chat-box") as HTMLElement | null;
    const rect = (section ?? element).getBoundingClientRect();
    return { rect, scale: Math.max(0.1, rect.width / Math.max(1, size.width)) };
  };
  return (
    <button
      type="button"
      className="chat-resize"
      aria-label={`Resize chat (${size.width} × ${size.height}); arrow keys change it`}
      title="Drag to resize"
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.setPointerCapture(event.pointerId);
        const { rect, scale } = box(event.currentTarget);
        start.current = { x: event.clientX, y: event.clientY, size, rect, scale };
      }}
      onPointerMove={(event) => {
        const from = start.current;
        if (!from) return;
        const next = draftFor(
          event.currentTarget,
          from.rect,
          {
            width: from.size.width + (event.clientX - from.x) / from.scale,
            height: from.size.height + (event.clientY - from.y) / from.scale,
          },
          from.scale,
        );
        latest.current = next;
        onDraft(next);
      }}
      onPointerUp={() => {
        const done = latest.current;
        start.current = latest.current = null;
        if (done) onCommit(done);
        else onDraft(null);
      }}
      onPointerCancel={() => {
        start.current = latest.current = null;
        onDraft(null);
      }}
      onLostPointerCapture={() => {
        // Lost without a release (say, the window lost focus): keep the size.
        if (!start.current) return;
        start.current = latest.current = null;
        onDraft(null);
      }}
      onKeyDown={(event) => {
        const step = event.shiftKey ? 40 : 10;
        const change: Record<string, [number, number]> = {
          ArrowLeft: [-step, 0],
          ArrowRight: [step, 0],
          ArrowUp: [0, -step],
          ArrowDown: [0, step],
        };
        const delta = change[event.key];
        if (!delta) return;
        event.preventDefault();
        event.stopPropagation();
        const { rect, scale } = box(event.currentTarget);
        onCommit(
          draftFor(
            event.currentTarget,
            rect,
            { width: size.width + delta[0], height: size.height + delta[1] },
            scale,
          ),
        );
      }}
    />
  );
}

function Box({
  queue,
  editing,
  size,
  resize,
}: {
  queue: Queue;
  editing: boolean;
  size: ChatSize;
  resize: { draft: (draft: ChatDraft | null) => void; commit: (draft: ChatDraft) => void };
}) {
  const chat = useStore(queue.chat, (c) => c);
  const spectating = useStore(engine, (s) => Boolean(s?.spectating));
  const seated = useStore(engine, (s) => Boolean(s?.networked && !s.spectating));
  const account = useStore(accountService, (a) => a);
  const me = useStore(account?.state ?? NO_ACCOUNT, (s) => s.me?.handle ?? null);
  const tabs = chatTabs(spectating);
  const active: ChatChannel = tabs.includes(chat.active) ? chat.active : tabs[0];
  const [draft, setDraft] = useState("");
  const [menu, setMenu] = useState<number | null>(null);
  const [focused, setFocused] = useState(false);
  const [needsTarget, setNeedsTarget] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const log = useRef<HTMLOListElement>(null);
  const focusRequest = useStore(chatFocusRequest, (r) => r);
  const lines = tabMessages(chat, active).slice(-SHOWN);
  // The tab follows where the player is: a seated player on Team (players
  // in a live match never hear Public), a spectator on Spectators (the only
  // one they post to), and anyone else on Public. Only on a change, so a
  // tab picked by hand stays.
  const place: ChatChannel = spectating ? "spectator" : seated ? "team" : "public";
  const wasPlace = useRef<ChatChannel | null>(null);
  useEffect(() => {
    const before = wasPlace.current;
    wasPlace.current = place;
    if (before === null ? place !== "public" : before !== place) queue.showChat(place);
  }, [queue, place]);
  // Enter (from the input router) opens the field: requests after this box
  // appeared, never the ones before it (a remount must not take the keys).
  const seenRequest = useRef(focusRequest.count);
  useEffect(() => {
    if (focusRequest.count > seenRequest.current) {
      // `/` starts a command (keeping a draft already begun with one).
      // `/` starts a command in an empty field; a draft already typed stays.
      if (focusRequest.prefill) setDraft((d) => d || focusRequest.prefill);
      field.current?.focus();
    }
    seenRequest.current = focusRequest.count;
  }, [focusRequest]);
  // Keep the newest line in view, unless the reader scrolled up.
  const newest = lines.at(-1)?.id ?? 0;
  const pinned = useRef(true);
  useEffect(() => {
    const element = log.current;
    if (element && pinned.current) element.scrollTop = element.scrollHeight;
  }, [newest, active]);
  const { text, tooLong } = chatText(draft);
  const whisperTo = chat.whisperTo;
  const leave = () => field.current?.blur();
  const send = () => {
    if (!text) {
      leave();
      return;
    }
    if (tooLong) return;
    // Commands run here (`/help` and the rest); `//` sends a line with one `/`.
    if (runSlash(text, pageCommandContext(queue))) {
      setDraft("");
      leave();
      return;
    }
    const message = unescapeSlash(text);
    if (active === "whisper") {
      if (!whisperTo || !isHandle(whisperTo)) {
        setNeedsTarget(true);
        return;
      }
      queue.whisper(whisperTo, message);
    } else queue.say(active, message);
    setDraft("");
    leave();
  };
  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    // Enter while an input method composes a character is not a send.
    if (event.key === "Enter" && !event.nativeEvent.isComposing) {
      event.preventDefault();
      send();
    } else if (event.key === "Escape") {
      // Leave the field, keeping the draft; the game has the keys again.
      event.preventDefault();
      event.stopPropagation();
      leave();
    }
  };
  return (
    <section
      className={`chat-box${focused ? " typing" : ""}`}
      aria-label="Chat"
      data-keyboard-owner={focused ? "true" : undefined}
      style={{ width: size.width, height: size.height }}
    >
      <div className="chat-tabs" role="tablist" aria-label="Chat channels">
        {tabs.map((tab) => {
          const unread = chat.unread[tab] ?? 0;
          return (
            <button
              key={tab}
              type="button"
              role="tab"
              aria-selected={tab === active}
              className="chat-tab"
              onClick={() => queue.showChat(tab)}
            >
              {chatTabLabel(tab)}
              {unread > 0 && tab !== active && (
                <span className="chat-unread" aria-label={`${unread} unread`}>
                  {unread > 99 ? "99+" : unread}
                </span>
              )}
            </button>
          );
        })}
      </div>
      <ol
        className="chat-log"
        ref={log}
        onScroll={(event) => {
          const element = event.currentTarget;
          pinned.current = element.scrollTop + element.clientHeight >= element.scrollHeight - 4;
        }}
        role="log"
        aria-live="polite"
        aria-label={chatTabLabel(active)}
      >
        {editing && lines.length === 0 ? (
          <li className="chat-line public">
            <span className="chat-name">[Public] A player</span>
            <span className="chat-text">: chat appears here</span>
          </li>
        ) : (
          lines.map((message) => (
            <Line
              key={message.id}
              queue={queue}
              message={message}
              ignored={(handle) => isIgnored(chat, handle)}
              menu={menu === message.id}
              setMenu={setMenu}
              me={me}
            />
          ))
        )}
      </ol>
      {chat.error && (
        <p className="chat-error" role="alert">
          {chatErrorText(chat.error)}{" "}
          <button type="button" onClick={() => queue.dismissChatError()}>
            OK
          </button>
        </p>
      )}
      {active === "whisper" && (
        <label className="chat-to">
          To
          <input
            value={whisperTo ?? ""}
            maxLength={21}
            placeholder="Name#number"
            aria-label="Whisper to"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setNeedsTarget(false);
              queue.whisperTo(event.target.value.trim() || null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                field.current?.focus();
              } else if (event.key === "Escape") {
                event.preventDefault();
                event.currentTarget.blur();
              }
            }}
          />
        </label>
      )}
      {needsTarget && active === "whisper" && (
        <p className="chat-error" role="status">
          Pick who to whisper: their name and number, or Whisper from a friend's name.
        </p>
      )}
      <input
        ref={field}
        className="chat-input"
        value={draft}
        placeholder={focused ? `Say in ${chatTabLabel(active)}` : "Press Enter to chat"}
        aria-label={`Message to ${chatTabLabel(active)}`}
        aria-invalid={tooLong || undefined}
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={keys}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
      />
      {tooLong && (
        <p className="chat-error" role="status">
          Too long: {MAX_CHAT_CHARS} characters at most.
        </p>
      )}
      {!editing && <ResizeGrip size={size} onDraft={resize.draft} onCommit={resize.commit} />}
    </section>
  );
}

/** The movable chat box (planning/issues/chat.md): channel tabs with unread
 * markers, the door's messages and refusals, whispers, and ignoring from a
 * name. Enter opens it; while typing, keys go to it, not to combat. Placed
 * with the layout editor like the unit frames. */
export function ChatBox() {
  const queue = useStore(matchQueue, (q) => q);
  const editing = useStore(interaction, (s) => s.layout);
  const saved = useConfigSelector((c) => c.frames.chat);
  const savedSize = useConfigSelector((c) => c.chatSize);
  // While the grip is dragged: the size, and the layout keeping the box's
  // top-left corner where it was; saved once, on release.
  const [draft, setDraft] = useState<ChatDraft | null>(null);
  // Layout editing takes over the frame: drop any resize in progress.
  useEffect(() => {
    if (editing) setDraft(null);
  }, [editing]);
  const size = draft?.size ?? savedSize;
  const keep = (next: ChatDraft) =>
    anchorAt(
      { ...saved, scale: next.scale },
      next.left,
      next.top,
      { width: innerWidth - 24, height: innerHeight - 24 },
      next.size,
    );
  const value = draft ? { ...keep(draft), scale: saved.scale } : saved;
  const shown = !!queue && (saved.visible || editing);
  useEffect(() => {
    chatShown.set(shown && saved.visible);
    return () => chatShown.set(false);
  }, [shown, saved.visible]);
  // An unconnected queue still shows the box: sending opens the connection.
  if (!queue) return null;
  return (
    <Placed
      id="chat"
      value={value}
      onChange={(next) => updateConfig((c) => ({ ...c, frames: { ...c.frames, chat: next } }))}
    >
      <Box
        queue={queue}
        editing={editing}
        size={size}
        resize={{
          draft: setDraft,
          commit: (next) => {
            setDraft(null);
            updateConfig((c) => ({
              ...c,
              chatSize: next.size,
              frames: { ...c.frames, chat: { ...keep(next), scale: c.frames.chat.scale } },
            }));
          },
        }}
      />
    </Placed>
  );
}
