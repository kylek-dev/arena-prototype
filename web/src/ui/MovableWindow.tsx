import { useState, type ReactNode } from "react";
import { Button, Heading } from "@radix-ui/themes";
import { layout } from "../domain/layout/model";
import { useMovable } from "./useMovable";

export function MovableWindow({
  title,
  children,
  close,
}: {
  title: string;
  children: ReactNode;
  close: () => void;
}) {
  const [value, setValue] = useState(() => layout(0.5, 0.03));
  const placement = useMovable(value, setValue, true);

  return (
    <aside
      ref={placement.ref}
      className="movable-window"
      style={placement.style}
      aria-label={title}
      data-keyboard-owner="true"
    >
      <div className="window-heading">
        <div
          className="window-grip"
          role="button"
          tabIndex={0}
          aria-label={`Move ${title}`}
          {...placement.dragProps}
        >
          <Heading size="3">{title}</Heading>
        </div>
        <Button variant="ghost" color="gray" aria-label={`Close ${title}`} onClick={close}>
          ×
        </Button>
      </div>
      {children}
    </aside>
  );
}
