import * as Tooltip from "@radix-ui/react-tooltip";
import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";

const Suppressed = createContext(false);

export function TooltipProvider({
  children,
  disabled = false,
  delayDuration = 300,
}: {
  children: ReactNode;
  disabled?: boolean;
  delayDuration?: number;
}) {
  return (
    <Suppressed.Provider value={disabled}>
      <Tooltip.Provider delayDuration={delayDuration}>{children}</Tooltip.Provider>
    </Suppressed.Provider>
  );
}

// The child must forward its ref and event props (a native button works directly).
// Content is descriptive, not interactive; use a popover for links or controls.
export function Hint({
  children,
  text,
  side = "top",
}: {
  children: ReactElement;
  text: ReactNode;
  side?: Tooltip.TooltipContentProps["side"];
}) {
  const disabled = useContext(Suppressed);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);
  return (
    <Tooltip.Root open={!disabled && open} onOpenChange={(next) => setOpen(!disabled && next)}>
      <Tooltip.Trigger asChild>{children}</Tooltip.Trigger>
      <Tooltip.Portal container={document.getElementById("overlay-theme") ?? undefined}>
        <Tooltip.Content className="tooltip" side={side} sideOffset={8} collisionPadding={12}>
          {text}
          <Tooltip.Arrow className="tooltip-arrow" />
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

export function TooltipCard({
  title,
  shortcut,
  description,
  facts,
  status,
  footer,
}: {
  title: ReactNode;
  shortcut?: ReactNode;
  description?: ReactNode;
  facts?: readonly { label: string; value: ReactNode }[];
  status?: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <div className="tooltip-card">
      <div className="tooltip-heading">
        <strong>{title}</strong>
        {shortcut && <span className="tooltip-shortcut">{shortcut}</span>}
      </div>
      {description && <p className="tooltip-description">{description}</p>}
      {facts && (
        <dl className="tooltip-facts">
          {facts.map(({ label, value }) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      )}
      {status && <p className="tooltip-status">{status}</p>}
      {footer && <p className="tooltip-footer">{footer}</p>}
    </div>
  );
}
