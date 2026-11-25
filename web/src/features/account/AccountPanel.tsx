import { Button, Text } from "@radix-ui/themes";
import { useState } from "react";
import { LOGIN_URL, PRIVACY_URL, SIGN_UP_URL, accountErrorText } from "../../domain/account";
import { LOADING_ACCOUNT } from "../../runtime/account";
import { accountService } from "../../state/account";
import { createStore, useStore } from "../../state/store";
import "./account.css";

const NO_ACCOUNT = createStore(LOADING_ACCOUNT);

function PrivacyLink() {
  return (
    <a className="account-privacy" href={PRIVACY_URL} target="_blank" rel="noopener">
      Privacy notice
    </a>
  );
}

/** Login and account status in Play online (social-identity, ADR 0055).
 * Shows nothing where the door offers no login, or cannot be reached. */
export function AccountPanel() {
  const account = useStore(accountService, (a) => a);
  const state = useStore(account?.state ?? NO_ACCOUNT, (s) => s);
  const [confirming, setConfirming] = useState(false);
  if (!account || state.status !== "ready" || !state.me?.login) return null;
  const { me, busy, error } = state;
  const notice = error && (
    <div className="account-error" role="alert">
      <Text size="1">{accountErrorText(error)}</Text>
      <Button size="1" variant="soft" color="gray" onClick={() => account.dismiss()}>
        OK
      </Button>
    </div>
  );
  if (!me.account)
    return (
      <div className="account" role="group" aria-label="Account">
        <Text size="1" color="gray">
          Playing as a guest. Log in to keep a name, add friends and chat.
        </Text>
        <div className="account-actions">
          <Button size="1" asChild>
            <a href={LOGIN_URL}>Log in</a>
          </Button>
          <Button size="1" variant="surface" asChild>
            <a href={SIGN_UP_URL}>Sign up</a>
          </Button>
          <PrivacyLink />
        </div>
        {notice}
      </div>
    );
  return (
    <div className="account" role="group" aria-label="Account">
      <Text size="1">
        {me.handle ? (
          <>
            Logged in as <strong className="account-handle">{me.handle}</strong>
          </>
        ) : (
          "Logged in. You have no name yet, so others see you as a guest."
        )}
      </Text>
      {confirming ? (
        <div className="account-confirm" role="alertdialog" aria-label="Delete account">
          <Text size="1">
            Delete your account? Your name, friends and blocks are removed here, and your login is
            removed at WorkOS. This cannot be undone.
          </Text>
          <div className="account-actions">
            <Button
              size="1"
              color="red"
              disabled={!!busy}
              onClick={async () => {
                await account.deleteAccount();
                // On a refusal the question stays open, so retrying is one click.
                if (!account.state.get().error) setConfirming(false);
              }}
            >
              {busy === "delete" ? "Deleting…" : "Delete account"}
            </Button>
            <Button
              size="1"
              variant="soft"
              color="gray"
              disabled={!!busy}
              onClick={() => setConfirming(false)}
            >
              Keep account
            </Button>
          </div>
        </div>
      ) : (
        <div className="account-actions">
          <Button
            size="1"
            variant="soft"
            color="gray"
            disabled={!!busy}
            onClick={() => account.logout()}
          >
            Log out
          </Button>
          <Button
            size="1"
            variant="soft"
            color="gray"
            disabled={!!busy}
            title="Ends your sessions on every device and browser"
            onClick={() => account.logoutEverywhere()}
          >
            Log out everywhere
          </Button>
          <Button
            size="1"
            variant="soft"
            color="red"
            disabled={!!busy}
            onClick={() => {
              account.dismiss();
              setConfirming(true);
            }}
          >
            Delete account…
          </Button>
          <PrivacyLink />
        </div>
      )}
      {notice}
    </div>
  );
}
