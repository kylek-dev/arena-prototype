import { Button, Text } from "@radix-ui/themes";
import { useState } from "react";
import {
  IDLE_QUEUE,
  QUEUE_MODES,
  endedText,
  inviteLink,
  modeLabel,
  noticeText,
  queueBlocked,
  type QueueMode,
} from "../../domain/matchmaking";
import type { Queue } from "../../runtime/matchmaking";
import { matchQueue } from "../../state/network";
import { createStore, useStore } from "../../state/store";
import "./play-online.css";

const NO_QUEUE = createStore(IDLE_QUEUE);
const REFUSALS = new Set([
  "not_leader",
  "party_too_big",
  "party_full",
  "party_busy",
  "no_such_party",
]);

function InviteLink({ code }: { code: string }) {
  const link = inviteLink(window.location, code);
  const [copied, setCopied] = useState<"yes" | "failed" | null>(null);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(link);
      setCopied("yes");
    } catch {
      setCopied("failed");
    }
  };
  return (
    <div className="stack">
      <div className="party-invite">
        <Button size="1" variant="soft" onClick={copy}>
          {copied === "yes" ? "Link copied" : "Copy invite link"}
        </Button>
      </div>
      {copied === "failed" && (
        <input
          className="party-link"
          readOnly
          value={link}
          aria-label="Invite link"
          onFocus={(event) => event.currentTarget.select()}
        />
      )}
    </div>
  );
}

function JoinParty({ queue }: { queue: Queue }) {
  const [open, setOpen] = useState(false);
  const [code, setCode] = useState("");
  if (!open)
    return (
      <Button size="1" variant="surface" color="gray" onClick={() => setOpen(true)}>
        Join a party
      </Button>
    );
  const join = () => {
    if (queue.joinParty(code)) {
      setOpen(false);
      setCode("");
    }
  };
  return (
    <div className="party-join">
      <input
        value={code}
        maxLength={12}
        placeholder="Code"
        aria-label="Party code"
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => setCode(event.target.value.toUpperCase())}
        onKeyDown={(event) => event.key === "Enter" && join()}
      />
      <Button size="1" onClick={join} disabled={!code.trim()}>
        Join
      </Button>
      <Button size="1" variant="ghost" color="gray" onClick={() => setOpen(false)}>
        Cancel
      </Button>
    </div>
  );
}

/** Party and queue controls: invite or join friends, then search. */
export function PlayOnline({ onQueue }: { onQueue?: () => void }) {
  const queue = useStore(matchQueue, (q) => q);
  const state = useStore(queue?.state ?? NO_QUEUE, (s) => s);
  if (!queue) return null;
  const party = state.party?.code ? state.party : null;
  const searching = state.phase !== "idle";
  // Answers to this panel's own requests; the queue bar tells the rest.
  const notice = REFUSALS.has(state.notice ?? "") ? noticeText(state.notice) : null;
  const start = (mode: QueueMode) => {
    queue.queue(mode);
    onQueue?.();
  };
  return (
    <section className="stack play-online" aria-labelledby="play-online-heading">
      <Text id="play-online-heading" size="2" weight="medium">
        Play online
      </Text>
      {state.connection === "ended" && (
        <div className="play-online-ended" role="alert">
          <Text size="1">{endedText(state.ended)}</Text>
          <Button size="1" variant="soft" color="gray" onClick={() => queue.dismiss()}>
            OK
          </Button>
        </div>
      )}
      <div className="party" role="group" aria-label="Party">
        {party ? (
          <>
            <Text size="1">
              Party <strong className="party-code">{party.code}</strong> · {party.members} of 3 ·{" "}
              {party.leader ? "you lead" : "a friend leads"}
            </Text>
            <div className="party-actions">
              {party.code && <InviteLink code={party.code} />}
              <Button
                size="1"
                variant="soft"
                color="gray"
                disabled={searching}
                onClick={() => queue.leaveParty()}
              >
                Leave party
              </Button>
            </div>
          </>
        ) : (
          <>
            <Text size="1" color="gray">
              Queue alone, or invite friends to play on your team.
            </Text>
            <div className="party-actions">
              <Button
                size="1"
                variant="surface"
                disabled={searching}
                onClick={() => queue.createParty()}
              >
                Invite friends
              </Button>
              {!searching && <JoinParty queue={queue} />}
            </div>
          </>
        )}
      </div>
      {notice && (
        <Text size="1" className="play-online-notice" role="status">
          {notice}
        </Text>
      )}
      {searching && state.mode ? (
        <div className="stack">
          <Text size="1" color="gray">
            {state.phase === "ready"
              ? "Your match is ready: accept it in the window."
              : `In the ${modeLabel(state.mode)} queue. The bar at the top shows your wait; you can keep practicing or customizing meanwhile.`}
          </Text>
          {state.phase === "queued" && (
            <Button variant="soft" color="gray" onClick={() => queue.cancel()}>
              Leave queue
            </Button>
          )}
        </div>
      ) : party && !party.leader ? (
        <Text size="1" color="gray">
          The party leader chooses the queue.
        </Text>
      ) : (
        <div className="queue-modes">
          {QUEUE_MODES.map((mode) => {
            const blocked = queueBlocked(state, mode.id);
            return (
              <Button
                key={mode.id}
                variant="surface"
                disabled={!!blocked}
                title={blocked ?? mode.hint}
                onClick={() => start(mode.id)}
              >
                {mode.label}
              </Button>
            );
          })}
        </div>
      )}
    </section>
  );
}
