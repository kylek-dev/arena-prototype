import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  MAX_CHAT_CHARS,
  chatErrorText,
  chatPrefix,
  chatTabLabel,
  chatTabs,
  defaultChannel,
  tabPartner,
  whisperDraft,
  whisperQuery,
  whisperSuggestions,
  chatText,
  isIgnored,
  tabMessages,
  type ChatMessage,
  type ChatTab,
} from "../../domain/chat";
import { handleName } from "../../domain/handles";
import type { Queue } from "../../runtime/matchmaking";
import { chatFocusRequest, chatShown, requestChat } from "../../state/chat";
import { updateConfig, useConfigSelector } from "../../state/config";
import { anchorAt, besideLauncher } from "../../domain/layout/model";
import { CHAT_SIZE, clampChatSize, type ChatSize } from "../../domain/preferences/schema";
import { matchQueue } from "../../state/network";
import { engine, interaction } from "../../state/session";
import { createStore, useStore } from "../../state/store";
import { accountService, friendsService } from "../../state/account";
import { NO_FRIENDS } from "../../runtime/friends";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { pageCommandContext, runSlash } from "../../runtime/chat-commands";
import { parseSlash, unescapeSlash } from "../../domain/commands";
import { AdminTag } from "../../ui/AdminTag";
import { Placed } from "../../ui/Placed";
import "./chat.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);
const NO_FRIENDS_LIST = createStore(NO_FRIENDS);
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
          // Starts a whisper in the field: `/w Name#0000 `.
          requestChat(`/w ${handle} `);
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
  const friends = useStore(friendsService, (f) => f);
  const tabs = chatTabs(spectating, chat.conversations);
  const active: ChatTab = tabs.includes(chat.active) ? chat.active : "all";
  const [draft, setDraft] = useState("");
  const [menu, setMenu] = useState<number | null>(null);
  const [focused, setFocused] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const log = useRef<HTMLOListElement>(null);
  const focusRequest = useStore(chatFocusRequest, (r) => r);
  const lines = tabMessages(chat, active).slice(-SHOWN);
  // Enter (from the input router) opens the field: requests after this box
  // appeared, never the ones before it (a remount must not take the keys).
  const seenRequest = useRef(focusRequest.count);
  useEffect(() => {
    if (focusRequest.count > seenRequest.current) {
      const prefill = focusRequest.prefill;
      // A whisper (`/w Name#0000 `) replaces the field; `/` starts a command
      // in an empty field and keeps a draft already typed.
      // Whisper keeps a message already typed, behind `/w Name#0000 `.
      if (prefill.startsWith("/w "))
        setDraft((d) => (d.trim() && !d.trimStart().startsWith("/") ? prefill + d : prefill));
      else if (prefill) setDraft((d) => d || prefill);
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
  // Who the field whispers to: `/w Name#0000 ` typed, or the conversation
  // tab shown (unless a command is being typed).
  // While a `/w` handle is typed: friends (those on first) and
  // conversation partners whose handle starts with it; Tab completes.
  const friendList = useStore(friends?.state ?? NO_FRIENDS_LIST, (f) => f.list);
  const candidates = useMemo(
    () => [
      ...(friendList?.friends ?? [])
        .filter((friend) => friend.relation === "friend")
        .map((friend) => ({ handle: friend.handle, online: friend.presence !== "offline" })),
      ...chat.conversations.map((handle) => ({ handle, online: false })),
    ],
    [friendList, chat.conversations],
  );
  const query = whisperQuery(draft);
  const suggestions = query ? whisperSuggestions(candidates, query.prefix) : [];
  const [pick, setPick] = useState(0);
  const picked = suggestions.length ? Math.min(pick, suggestions.length - 1) : -1;
  const complete = (handle: string) => {
    if (!query) return;
    setDraft(`/${query.command} ${handle} `);
    setPick(0);
    field.current?.focus();
  };
  const typed = whisperDraft(draft);
  const partner = tabPartner(active);
  // Only a real command leaves the conversation; `//text` is a line too.
  const whisperingTo = typed?.to ?? (partner && !parseSlash(draft) ? partner : null);
  const body = typed ? typed.text : draft;
  const { text, tooLong } = chatText(body);
  const channel = defaultChannel({ seated, spectating });
  /** The channel plain text goes to: All's default, or the tab's own. */
  const target =
    active === "all"
      ? channel
      : partner
        ? null
        : (active as Exclude<ChatTab, "all" | `w:${string}`>);
  const leave = () => field.current?.blur();
  const send = () => {
    if (!text) {
      if (!typed) leave();
      return;
    }
    if (tooLong) return;
    if (whisperingTo) {
      queue.whisper(whisperingTo, typed ? text : unescapeSlash(text));
    } else {
      // Commands run here (`/help` and the rest); `//` sends one `/`.
      if (runSlash(text, pageCommandContext(queue))) {
        setDraft("");
        leave();
        return;
      }
      const message = unescapeSlash(text);
      queue.say(target ?? channel, message);
    }
    setDraft("");
    leave();
  };
  const keys = (event: KeyboardEvent<HTMLInputElement>) => {
    // Whisper suggestions: Tab (or Enter) completes, arrows choose.
    if (suggestions.length && !event.nativeEvent.isComposing) {
      if (event.key === "Tab" || event.key === "Enter") {
        event.preventDefault();
        complete(suggestions[picked]);
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : -1;
        setPick((picked + step + suggestions.length) % suggestions.length);
        return;
      }
    }
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
          const handle = tabPartner(tab);
          return (
            <span key={tab} className={`chat-tab-wrap${handle ? " conversation" : ""}`}>
              <button
                type="button"
                role="tab"
                aria-selected={tab === active}
                className="chat-tab"
                title={handle ?? undefined}
                onClick={() => queue.showChat(tab)}
              >
                {handle ? handleName(handle) : chatTabLabel(tab)}
                {unread > 0 && tab !== active && (
                  <span className="chat-unread" aria-label={`${unread} unread`}>
                    {unread > 99 ? "99+" : unread}
                  </span>
                )}
              </button>
              {handle && (
                <button
                  type="button"
                  className="chat-tab-close"
                  aria-label={`Close the conversation with ${handle}`}
                  onClick={() => queue.closeConversation(handle)}
                >
                  ×
                </button>
              )}
            </span>
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
        aria-label={partner ?? chatTabLabel(active)}
      >
        {editing && lines.length === 0 ? (
          <li className="chat-line public">
            <span className="chat-name">[Public] A player</span>
            <span className="chat-text">: Chat messages appear here.</span>
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
      {focused && suggestions.length > 0 && (
        <ul className="chat-suggestions" role="listbox" aria-label="Whisper suggestions">
          {suggestions.map((handle, index) => (
            <li
              key={handle}
              role="option"
              aria-selected={index === picked}
              className="chat-suggestion"
              // Pressing keeps the field focused; the click completes.
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => complete(handle)}
            >
              {handle}
            </li>
          ))}
        </ul>
      )}
      {whisperingTo && (
        <p className="chat-whispering" role="status">
          Whispering to {whisperingTo}
        </p>
      )}
      <input
        ref={field}
        className={`chat-input${whisperingTo ? " whispering" : ""}`}
        value={draft}
        placeholder={
          focused
            ? whisperingTo
              ? `Whisper to ${whisperingTo}`
              : `Say in ${chatTabLabel(target ?? channel)}`
            : "Press Enter to chat"
        }
        aria-label={
          whisperingTo
            ? `Whisper to ${whisperingTo}`
            : `Message to ${chatTabLabel(target ?? channel)}`
        }
        aria-invalid={tooLong || undefined}
        autoComplete="off"
        spellCheck={false}
        aria-autocomplete={query ? "list" : undefined}
        aria-expanded={query ? suggestions.length > 0 : undefined}
        onChange={(event) => {
          setDraft(event.target.value);
          setPick(0);
        }}
        onKeyDown={keys}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
      />
      {tooLong && (
        <p className="chat-error" role="status">
          Messages hold up to {MAX_CHAT_CHARS} characters.
        </p>
      )}
      {!editing && <ResizeGrip size={size} onDraft={resize.draft} onCommit={resize.commit} />}
    </section>
  );
}

/** The launcher's place and the action bar's left edge, measured on screen
 * (frames are positioned by fractions, so only the DOM knows where the
 * launcher really is); `null` when not attached, nothing is measurable or
 * there is no room beside the launcher. Watches the launcher and the bars
 * themselves (size and style), so the chat follows them as they move. */
function useBesideLauncher(
  attached: boolean,
  height: number,
  scale: number,
  watch: readonly unknown[],
) {
  const [at, setAt] = useState<ReturnType<typeof besideLauncher> | null>(null);
  useLayoutEffect(() => {
    if (!attached) {
      setAt(null);
      return;
    }
    let frame = 0;
    const measure = () => {
      const placed = document.querySelector(".launcher")?.closest(".placed");
      const rect = placed?.getBoundingClientRect();
      if (!rect || !rect.width) {
        setAt(null);
        return;
      }
      // Only bars level with the chat (whose bottom meets the launcher's).
      const top = rect.bottom - height * scale;
      const lefts = [...document.querySelectorAll(".action-bar")]
        .map((bar) => (bar.closest(".placed") ?? bar).getBoundingClientRect())
        .filter(
          (bar) => bar.width && bar.left > rect.right && bar.top < rect.bottom && bar.bottom > top,
        )
        .map((bar) => bar.left);
      const next = besideLauncher(
        rect,
        lefts.length ? Math.min(...lefts) : null,
        height,
        scale,
        innerWidth,
        CHAT_SIZE.min.width,
      );
      setAt((before) =>
        before === next ||
        (before &&
          next &&
          before.left === next.left &&
          before.top === next.top &&
          before.width === next.width)
          ? before
          : next,
      );
    };
    const soon = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    };
    const targets = () =>
      [document.querySelector(".launcher"), ...document.querySelectorAll(".action-bar")].flatMap(
        (element) => {
          const placed = element?.closest(".placed") ?? element;
          return placed ? [placed] : [];
        },
      );
    const sizes = new ResizeObserver(soon);
    const moves = new MutationObserver(soon);
    for (const target of targets()) {
      sizes.observe(target);
      moves.observe(target, { attributes: true, attributeFilter: ["style"] });
    }
    measure();
    soon();
    addEventListener("resize", soon);
    return () => {
      cancelAnimationFrame(frame);
      sizes.disconnect();
      moves.disconnect();
      removeEventListener("resize", soon);
    };
  }, [attached, height, scale, ...watch]);
  return at;
}

/** The movable chat box (planning/issues/chat.md): channel tabs with unread
 * markers, the door's messages and refusals, whispers, and ignoring from a
 * name. Enter opens it; while typing, keys go to it, not to combat. Placed
 * with the layout editor, beside the launcher by default. */
export function ChatBox() {
  const queue = useStore(matchQueue, (q) => q);
  const editing = useStore(interaction, (s) => s.layout);
  const saved = useConfigSelector((c) => c.frames.chat);
  const savedSize = useConfigSelector((c) => c.chatSize);
  const attached = useConfigSelector((c) => c.chatAttached);
  const custom = useConfigSelector((c) => c.chatSizeCustom);
  const launcher = useConfigSelector((c) => c.frames.launcher);
  const bars = useConfigSelector((c) => c.bars);
  // Attached: measured beside the launcher (bottoms aligned), as wide as
  // the space up to the action bar unless the player sized it.
  const beside = useBesideLauncher(attached, savedSize.height, saved.scale, [launcher, bars]);
  // While the grip is dragged: the size, and the layout keeping the box's
  // top-left corner where it was; saved once, on release.
  const [draft, setDraft] = useState<ChatDraft | null>(null);
  // Layout editing takes over the frame: drop any resize in progress.
  useEffect(() => {
    if (editing) setDraft(null);
  }, [editing]);
  const size =
    draft?.size ??
    (beside && !custom && beside.width !== null
      ? clampChatSize({ width: beside.width, height: savedSize.height })
      : savedSize);
  const keep = (next: ChatDraft) =>
    anchorAt(
      { ...saved, scale: next.scale },
      next.left,
      next.top,
      { width: innerWidth - 24, height: innerHeight - 24 },
      next.size,
    );
  const value = draft
    ? { ...keep(draft), scale: saved.scale }
    : beside
      ? anchorAt(
          saved,
          beside.left,
          beside.top,
          { width: innerWidth - 24, height: innerHeight - 24 },
          size,
        )
      : saved;
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
      // Moving the box detaches it from the launcher, keeping the width
      // it showed.
      onChange={(next) =>
        updateConfig((c) => ({
          ...c,
          chatAttached: false,
          chatSize: size,
          frames: { ...c.frames, chat: next },
        }))
      }
    >
      <Box
        queue={queue}
        editing={editing}
        size={size}
        resize={{
          draft: setDraft,
          commit: (next) => {
            setDraft(null);
            // A resize keeps the player's size; an attached box stays
            // attached (its bottom-left corner follows the launcher).
            updateConfig((c) => ({
              ...c,
              chatSize: next.size,
              // Only a width change is the player's width; a taller box
              // still fills the space beside the launcher.
              chatSizeCustom: c.chatSizeCustom || next.size.width !== size.width,
              frames: c.chatAttached
                ? c.frames
                : { ...c.frames, chat: { ...keep(next), scale: c.frames.chat.scale } },
            }));
          },
        }}
      />
    </Placed>
  );
}
