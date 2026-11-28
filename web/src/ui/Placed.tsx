import type { CSSProperties, ReactNode } from "react";
import type { Layout } from "../domain/layout/model";
import { interaction, selectedLayout } from "../state/session";
import { useStore } from "../state/store";
import { useConfigSelector } from "../state/config";
import { useMovable } from "./useMovable";

export function Placed({
  id,
  value,
  onChange,
  children,
}: {
  id: string;
  value: Layout;
  onChange: (layout: Layout) => void;
  children: ReactNode;
}) {
  const editing = useStore(interaction, (s) => s.layout);
  const selected = useStore(selectedLayout, (s) => s === id);
  const zoom = useConfigSelector((c) => c.uiScale);
  const placement = useMovable(value, onChange, editing && !value.locked, zoom);

  if (!value.visible && !editing) return null;
  // Frames that draw a background read its opacity from this variable.
  const style: CSSProperties =
    value.background === undefined
      ? placement.style
      : ({
          ...placement.style,
          "--hud-background": `${Math.round(value.background * 100)}%`,
        } as CSSProperties);

  return (
    <div
      ref={placement.ref}
      className={`placed ${editing ? "editing" : ""} ${editing && selected ? "layout-selected" : ""} ${!value.visible ? "preview" : ""}`}
      style={style}
      data-background={value.background === 0 ? "none" : undefined}
      role={editing ? "button" : undefined}
      tabIndex={editing ? 0 : undefined}
      aria-label={editing ? `Edit ${id}` : undefined}
      aria-pressed={editing ? selected : undefined}
      {...placement.dragProps}
      onPointerDown={(e) => {
        if (!editing || e.button !== 0) return;
        selectedLayout.set(id);
        e.currentTarget.focus();
        placement.dragProps.onPointerDown(e);
      }}
      onFocus={() => {
        if (editing) selectedLayout.set(id);
      }}
      onClickCapture={(e) => {
        if (!editing) return;
        e.preventDefault();
        e.stopPropagation();
        selectedLayout.set(id);
      }}
    >
      <div inert={editing}>{children}</div>
    </div>
  );
}
