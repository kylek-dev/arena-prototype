import type { ReactNode } from "react";
import "./dock.css";

/** One window in the dock: the same size as every other, a title bar with a
 * close button, a scrolling body and an optional footer (a tab bar, say). */
export function DockWindow({
  title,
  close,
  children,
  footer,
  className = "",
  keyboardOwner = true,
}: {
  title: string;
  close: () => void;
  children: ReactNode;
  footer?: ReactNode;
  className?: string;
  /** Whether any focus inside keeps the keys from gameplay. The dock's
   * windows do not: their buttons and tabs leave the keys with the game,
   * and text fields take them by themselves (`isTextEntry`). */
  keyboardOwner?: boolean;
}) {
  return (
    <section
      className={`dock-window ${className}`}
      role="dialog"
      aria-modal="false"
      aria-label={title}
      data-keyboard-owner={keyboardOwner ? "true" : undefined}
    >
      <header className="dock-window-title">
        <h2>{title}</h2>
        <button
          type="button"
          className="dock-window-close"
          aria-label={`Close ${title}`}
          onClick={close}
        >
          ×
        </button>
      </header>
      <div className="dock-window-body">{children}</div>
      {footer && <footer className="dock-window-footer">{footer}</footer>}
    </section>
  );
}
