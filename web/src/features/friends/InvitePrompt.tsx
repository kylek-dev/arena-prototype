import { Button } from "@radix-ui/themes";
import { IDLE_QUEUE, challengeText, noticeText } from "../../domain/matchmaking";
import { matchQueue } from "../../state/network";
import { createStore, useStore } from "../../state/store";
import { useCenterFree } from "../center/CenterPanel";
import "./friends.css";

const NO_QUEUE = createStore(IDLE_QUEUE);

/** A friend's invite to their party or custom lobby, wherever the player
 * is: Accept joins it by code, Decline just dismisses (the inviter is not
 * told). If joining is refused, the reason shows here. Non-modal, so a
 * match in progress keeps its controls; announced politely.
 *
 * Challenges to a 1v1 (ADR 0060) share the place: another player's with
 * Accept (a match follows) and Decline (they are told), one's own while it
 * waits, with Withdraw, and what became of either. */
export function InvitePrompt() {
  const queue = useStore(matchQueue, (q) => q);
  const invites = useStore(queue?.state ?? NO_QUEUE, (s) => s.invites);
  const failed = useStore(queue?.state ?? NO_QUEUE, (s) => s.inviteError);
  const challenges = useStore(queue?.state ?? NO_QUEUE, (s) => s.challenges);
  const challenging = useStore(queue?.state ?? NO_QUEUE, (s) => s.challenging);
  const challengeNotice = useStore(queue?.state ?? NO_QUEUE, (s) => s.challengeNotice);
  const centered = useCenterFree();
  const waiting = challenging?.sent ? challenging : null;
  if (
    !queue ||
    (invites.length === 0 && !failed && challenges.length === 0 && !waiting && !challengeNotice)
  )
    return null;
  return (
    // Outside a match, cards in the center panel; in one, the right edge.
    <div className={`invite-prompts${centered ? " centered" : ""}`} aria-live="polite">
      {failed && (
        <section className="invite-prompt" role="group" aria-label="Invite could not be joined">
          <p>
            The invitation from {failed.from} could not be accepted.{" "}
            {noticeText(failed.code) ?? "The invitation has expired."}
          </p>
          <div className="invite-prompt-actions">
            <Button
              size="1"
              variant="surface"
              color="gray"
              className="action-button"
              onClick={() => queue.dismissInviteError()}
            >
              OK
            </Button>
          </div>
        </section>
      )}
      {challengeNotice && (
        <section className="invite-prompt" role="group" aria-label="Challenge notice">
          <p>{challengeText(challengeNotice)}</p>
          <div className="invite-prompt-actions">
            <Button
              size="1"
              variant="surface"
              color="gray"
              className="action-button"
              onClick={() => queue.dismissChallengeNotice()}
            >
              OK
            </Button>
          </div>
        </section>
      )}
      {waiting && (
        <section className="invite-prompt" role="group" aria-label={`Challenge to ${waiting.to}`}>
          <p>
            Your challenge to <strong>{waiting.to}</strong> is waiting for an answer.
          </p>
          <div className="invite-prompt-actions">
            <Button
              size="1"
              variant="surface"
              color="gray"
              className="action-button"
              onClick={() => queue.withdrawChallenge()}
            >
              Withdraw
            </Button>
          </div>
        </section>
      )}
      {challenges.map((challenge) => (
        <section
          key={`challenge-${challenge.from}`}
          className="invite-prompt"
          role="group"
          aria-label={`Challenge from ${challenge.from}`}
        >
          <p>
            <strong>{challenge.from}</strong> challenges you to a 1v1.
          </p>
          <div className="invite-prompt-actions">
            <Button
              size="1"
              className="action-button"
              disabled={challenge.answering}
              onClick={() => queue.answerChallenge(challenge.from, true)}
            >
              {challenge.answering ? "Accepted" : "Accept"}
            </Button>
            <Button
              size="1"
              variant="surface"
              color="gray"
              className="action-button"
              disabled={challenge.answering}
              onClick={() => queue.answerChallenge(challenge.from, false)}
            >
              Decline
            </Button>
          </div>
        </section>
      ))}
      {invites.map((invite) => (
        <section
          key={`${invite.from}-${invite.code}`}
          className="invite-prompt"
          role="group"
          aria-label={`Invite from ${invite.from}`}
        >
          <p>
            <strong>{invite.from}</strong> invites you to{" "}
            {invite.kind === "lobby" ? "their custom lobby" : "their party"}.
          </p>
          <div className="invite-prompt-actions">
            <Button size="1" className="action-button" onClick={() => queue.acceptInvite(invite)}>
              Accept
            </Button>
            <Button
              size="1"
              variant="surface"
              color="gray"
              className="action-button"
              onClick={() => queue.declineInvite(invite)}
            >
              Decline
            </Button>
          </div>
        </section>
      ))}
    </div>
  );
}
