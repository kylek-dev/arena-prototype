import { Button, Text } from "@radix-ui/themes";
import { layout, type Layout } from "../../domain/layout/model";
import {
  CHAT_SIZE,
  clampChatSize,
  defaultConfig,
  type ChatSize,
  type FrameRole,
} from "../../domain/preferences/schema";
import { useConfigSelector, updateConfig } from "../../state/config";
import { interaction, selectedLayout, setMode } from "../../state/session";
import { useStore } from "../../state/store";
import { MovableWindow } from "../../ui/MovableWindow";

export function LayoutSettings() {
  const frames = useConfigSelector((c) => c.frames);
  const castBarAttached = useConfigSelector((c) => c.castBarAttached);
  const chatSize = useConfigSelector((c) => c.chatSize);
  const chatAttached = useConfigSelector((c) => c.chatAttached);
  const resizeChat = (next: ChatSize) =>
    updateConfig((c) => ({
      ...c,
      chatSize: clampChatSize(next),
      // A changed width or height is the player's own.
      chatSizeCustom: c.chatSizeCustom || next.width !== c.chatSize.width,
      chatHeightCustom: c.chatHeightCustom || next.height !== c.chatSize.height,
    }));
  const bars = useConfigSelector((c) => c.bars);
  const selected = useStore(selectedLayout, (s) => s);
  const editing = useStore(interaction, (s) => s.layout);
  const isFrame = selected !== null && Object.hasOwn(frames, selected);
  const value = isFrame
    ? frames[selected as FrameRole]
    : bars.find((b) => b.id === selected)?.layout;

  function change(next: Layout) {
    updateConfig((c) =>
      isFrame
        ? { ...c, frames: { ...c.frames, [selected!]: next } }
        : { ...c, bars: c.bars.map((b) => (b.id === selected ? { ...b, layout: next } : b)) },
    );
  }

  return (
    <div className="stack">
      <Text size="2" color="gray">
        Click a frame or bar to select it. Drag anywhere on it to move it; arrow keys also work.
        Drag this window by its title.
      </Text>
      {editing && value ? (
        <>
          <Text weight="medium">Editing {selected}</Text>
          <label>
            <input
              type="checkbox"
              checked={value.visible}
              onChange={(e) => change({ ...value, visible: e.target.checked })}
            />
            Visible
          </label>
          <label>
            <input
              type="checkbox"
              checked={value.locked}
              onChange={(e) => change({ ...value, locked: e.target.checked })}
            />
            Position locked
          </label>
          <label>
            Scale {Math.round(value.scale * 100)}%
            <input
              type="range"
              min={0.6}
              max={1.6}
              step={0.05}
              value={value.scale}
              onChange={(e) => change({ ...value, scale: +e.target.value })}
            />
          </label>
          {selected === "chat" && (
            <>
              <label>
                <input
                  type="checkbox"
                  checked={chatAttached}
                  onChange={(e) => updateConfig((c) => ({ ...c, chatAttached: e.target.checked }))}
                />
                Attach chat to the launcher
              </label>
              <label>
                Width {chatSize.width} px
                <input
                  type="range"
                  min={CHAT_SIZE.min.width}
                  max={CHAT_SIZE.max.width}
                  step={10}
                  value={chatSize.width}
                  onChange={(e) => resizeChat({ ...chatSize, width: +e.target.value })}
                />
              </label>
              <label>
                Height {chatSize.height} px
                <input
                  type="range"
                  min={CHAT_SIZE.min.height}
                  max={CHAT_SIZE.max.height}
                  step={10}
                  value={chatSize.height}
                  onChange={(e) => resizeChat({ ...chatSize, height: +e.target.value })}
                />
              </label>
            </>
          )}
          <Button
            variant="soft"
            onClick={() => {
              change(isFrame ? defaultConfig().frames[selected as FrameRole] : layout(0.5, 0.92));
              if (selected === "chat")
                updateConfig((c) => ({
                  ...c,
                  chatAttached: true,
                  chatSizeCustom: false,
                  chatHeightCustom: false,
                  chatSize: { ...CHAT_SIZE.initial },
                }));
            }}
          >
            Reset selected
          </Button>
        </>
      ) : (
        <Text size="2" color="gray">
          {editing ? "No frame selected." : "Open canvas editing to select frames directly."}
        </Text>
      )}
      <label>
        <input
          type="checkbox"
          checked={castBarAttached}
          onChange={(e) => {
            // The separate bar disappears when attached: drop a stale selection.
            if (e.target.checked && selectedLayout.get() === "castBar") selectedLayout.set(null);
            updateConfig((c) => ({ ...c, castBarAttached: e.target.checked }));
          }}
        />
        Attach player cast bar to the player frame
      </label>
      <Button
        variant="surface"
        color="gray"
        onClick={() =>
          updateConfig((c) => ({
            ...c,
            frames: defaultConfig().frames,
            bars: c.bars.map((b) => ({ ...b, layout: layout(0.5, 0.92) })),
          }))
        }
      >
        Reset all positions
      </Button>
      <Button onClick={() => setMode("layout")}>{editing ? "Done" : "Edit on canvas"}</Button>
    </div>
  );
}

export function LayoutToolbar() {
  const editing = useStore(interaction, (s) => s.layout);
  return (
    editing && (
      <MovableWindow title="Layout editor" close={() => setMode("layout")}>
        <LayoutSettings />
      </MovableWindow>
    )
  );
}
