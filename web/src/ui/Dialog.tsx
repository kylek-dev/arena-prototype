import * as Dialog from "@radix-ui/react-dialog";
import { useRef, type ReactNode } from "react";
import { Button, Heading } from "@radix-ui/themes";
export function Modal({
  title,
  description,
  children,
  close,
  escapeBlocked = false,
  modal = true,
  className = "",
}: {
  title: string;
  description?: string;
  children: ReactNode;
  close: () => void;
  escapeBlocked?: boolean;
  modal?: boolean;
  className?: string;
}) {
  const previousFocus = useRef(document.activeElement as HTMLElement | null);
  return (
    <Dialog.Root
      open
      modal={modal}
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <Dialog.Portal container={document.getElementById("overlay-theme") ?? undefined}>
        <Dialog.Overlay className="scrim" />
        <Dialog.Content
          className={`dialog ${modal ? "" : "floating-dialog"} ${className}`}
          data-keyboard-owner={modal ? "true" : undefined}
          onOpenAutoFocus={(event) => {
            if (!modal) event.preventDefault();
          }}
          onEscapeKeyDown={(e) => {
            if (escapeBlocked) e.preventDefault();
          }}
          onCloseAutoFocus={(e) => {
            e.preventDefault();
            const previous = previousFocus.current;
            if (previous?.isConnected) previous.focus();
            else document.getElementById("arena")?.focus();
          }}
          onPointerDownOutside={(e) => e.preventDefault()}
          onInteractOutside={(e) => e.preventDefault()}
        >
          <div className="dialog-heading">
            <Dialog.Title asChild>
              <Heading size="5">{title}</Heading>
            </Dialog.Title>
            <Dialog.Close asChild>
              <Button variant="ghost" color="gray" aria-label={`Close ${title}`}>
                ×
              </Button>
            </Dialog.Close>
          </div>
          <Dialog.Description>
            {description ?? "Local practice configuration. Changes apply immediately."}
          </Dialog.Description>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
