import { useRef, useState, type PointerEvent, type ReactNode } from "react";
import { Button } from "@radix-ui/themes";
import {
  chosen,
  chosenVariant,
  defaultAppearance,
  eyeColors,
  featureGroups,
  groupLabels,
  hairColors,
  outfitSlots,
  randomAppearance,
  selectKit,
  skinTones,
  tintsEyes,
  type Appearance,
  type Group,
  type Kit,
  type Swatch,
} from "../../domain/appearance";
import {
  appearance,
  appearanceSaveError,
  beforeMatch,
  characterKits,
  customizerVisible,
  enterArena,
  preview,
  updateAppearance,
} from "../../state/appearance";
import { dispatch } from "../../runtime/commands";
import { engine } from "../../state/session";
import { useStore } from "../../state/store";
import { ancestryNames, bodyNames, describe, optionName, variantName } from "./labels";
import "./customizer.css";

type Tab = "body" | "head" | "outfit";
const tabs: [Tab, string][] = [
  ["body", "Body"],
  ["head", "Head"],
  ["outfit", "Outfit"],
];
const materialLabels: Record<string, string> = {
  Ranger: "Ranger colours",
  Peasant: "Peasant colours",
  Hair_Fantasy: "Hair shade",
};

function Choices({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { id: string; label: string }[];
  value: string | undefined;
  onChange: (id: string) => void;
}) {
  return (
    <fieldset className="customizer-field">
      <legend>{label}</legend>
      <div className="customizer-choices" role="radiogroup" aria-label={label}>
        {options.map((option) => (
          <button
            key={option.id}
            type="button"
            role="radio"
            aria-checked={option.id === value}
            className="customizer-choice"
            onClick={() => onChange(option.id)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </fieldset>
  );
}

function Swatches({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: Swatch[];
  value: string;
  onChange: (id: string) => void;
}) {
  const current = options.find((o) => o.id === value);
  return (
    <fieldset className="customizer-field">
      <legend>
        {label} {current && <span className="customizer-current">{current.label}</span>}
      </legend>
      <div className="customizer-swatches" role="radiogroup" aria-label={label}>
        {options.map((option) => (
          <button
            key={option.id}
            type="button"
            role="radio"
            aria-checked={option.id === value}
            aria-label={option.label}
            title={option.label}
            className="customizer-swatch"
            style={{ background: `var(--pal-${option.color})` }}
            onClick={() => onChange(option.id)}
          />
        ))}
      </div>
    </fieldset>
  );
}

function Section({ children }: { children: ReactNode }) {
  return <div className="customizer-section stack">{children}</div>;
}

const unique = (values: string[]) => [...new Set(values)];

interface TabProps {
  kits: Kit[];
  kit: Kit;
  look: Appearance;
  set: (next: Appearance) => void;
}

const setGroup = (look: Appearance, group: Group, id: string): Appearance => ({
  ...look,
  groups: { ...look.groups, [group]: id },
});

function variantChoices(kit: Kit, material: string) {
  return (kit.materials[material]?.variants ?? []).map((v, i) => ({
    id: v.id,
    label: variantName(material, i, v.label),
  }));
}

function BodyTab({ kits, kit, look, set }: TabProps) {
  const bodies = unique(kits.map((k) => k.body));
  const ancestries = unique(kits.map((k) => k.ancestry));
  const skin = variantChoices(kit, "Skin");
  return (
    <Section>
      <Choices
        label="Body type"
        options={bodies.map((id) => ({ id, label: bodyNames[id] ?? id }))}
        value={kit.body}
        onChange={(body) => set({ ...look, body })}
      />
      {ancestries.length > 1 && (
        <Choices
          label="Ancestry"
          options={ancestries.map((id) => ({ id, label: ancestryNames[id] ?? id }))}
          value={kit.ancestry}
          onChange={(ancestry) => set({ ...look, ancestry })}
        />
      )}
      <Swatches
        label="Skin tone"
        options={skinTones}
        value={look.skinTone}
        onChange={(skinTone) => set({ ...look, skinTone })}
      />
      {skin.length > 1 && (
        <Choices
          label={kit.ancestry === "human" ? "Underwear" : "Complexion"}
          options={skin}
          value={chosenVariant(kit, look, "Skin")?.id}
          onChange={(id) => set({ ...look, variants: { ...look.variants, Skin: id } })}
        />
      )}
      {kit.morph && (
        <label className="customizer-field customizer-slider">
          <span>Build</span>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={look.build}
            onChange={(e) => set({ ...look, build: e.target.valueAsNumber })}
          />
          <span className="customizer-range-labels" aria-hidden="true">
            <span>Lean</span>
            <span>Heavy</span>
          </span>
        </label>
      )}
    </Section>
  );
}

function HeadTab({ kit, look, set }: TabProps) {
  const eyes = kit.materials.Eyes?.variants ?? [];
  const tintedEyes = tintsEyes(kit);
  const tintedBeard = Object.values(kit.materials).some((m) => m.tint === "facialHair");
  const hooded = kit.groups.head && chosen(kit, look, "head")?.id !== "none";
  const shades = variantChoices(kit, "Hair_Fantasy");
  return (
    <Section>
      {featureGroups.map(
        (group) =>
          kit.groups[group] && (
            <Choices
              key={group}
              label={groupLabels[group]}
              options={kit.groups[group].map((o) => ({ id: o.id, label: optionName(kit, o) }))}
              value={chosen(kit, look, group)?.id}
              onChange={(id) => set(setGroup(look, group, id))}
            />
          ),
      )}
      {hooded && <p className="customizer-note">Your hood covers your hair while worn.</p>}
      <Swatches
        label="Hair colour"
        options={hairColors}
        value={look.hairColor}
        onChange={(hairColor) =>
          // The beard follows the hair until it is given its own colour.
          set({
            ...look,
            hairColor,
            facialHairColor:
              look.facialHairColor === look.hairColor ? hairColor : look.facialHairColor,
          })
        }
      />
      {tintedBeard && (
        <Swatches
          label="Facial hair colour"
          options={hairColors}
          value={look.facialHairColor}
          onChange={(facialHairColor) => set({ ...look, facialHairColor })}
        />
      )}
      {shades.length > 1 && (
        <Choices
          label={materialLabels.Hair_Fantasy}
          options={shades}
          value={chosenVariant(kit, look, "Hair_Fantasy")?.id}
          onChange={(id) => set({ ...look, variants: { ...look.variants, Hair_Fantasy: id } })}
        />
      )}
      {tintedEyes && (
        <Swatches
          label="Eye colour"
          options={eyeColors}
          value={eyeColors.some((c) => c.id === look.eyes) ? look.eyes : kit.eyes}
          onChange={(eyes) => set({ ...look, eyes })}
        />
      )}
      {!tintedEyes && eyes.length > 0 && (
        <Choices
          label="Eyes"
          options={eyes}
          value={chosenVariant(kit, look, "Eyes")?.id}
          onChange={(eyes) => set({ ...look, eyes })}
        />
      )}
    </Section>
  );
}

function OutfitTab({ kit, look, set }: TabProps) {
  const slots = outfitSlots.filter((slot) => kit.groups[slot]);
  if (!slots.length)
    return (
      <Section>
        <p className="customizer-note">No outfits are available for this ancestry yet.</p>
      </Section>
    );
  const sets = unique(
    slots.flatMap((slot) => kit.groups[slot].map((o) => o.id)).filter((id) => id !== "none"),
  );
  const variants = sets.filter((name) => (kit.materials[name]?.variants?.length ?? 0) > 1);
  return (
    <Section>
      <div className="customizer-sets">
        {sets.map((name) => (
          <Button
            key={name}
            variant="soft"
            size="1"
            onClick={() => {
              let next = look;
              for (const slot of slots)
                if (slot !== "head" && kit.groups[slot].some((o) => o.id === name))
                  next = setGroup(next, slot, name);
              set(next);
            }}
          >
            {optionName(kit, { id: name, label: name })} set
          </Button>
        ))}
        <Button
          variant="soft"
          color="gray"
          size="1"
          onClick={() => {
            let next = look;
            for (const slot of slots) next = setGroup(next, slot, "none");
            set(next);
          }}
        >
          Unequip all
        </Button>
      </div>
      {slots.map((slot) => (
        <Choices
          key={slot}
          label={groupLabels[slot]}
          options={kit.groups[slot].map((o) => ({ id: o.id, label: optionName(kit, o) }))}
          value={chosen(kit, look, slot)?.id}
          onChange={(id) => set(setGroup(look, slot, id))}
        />
      ))}
      {variants.map((name) => (
        <Choices
          key={name}
          label={materialLabels[name] ?? `${name} colours`}
          options={variantChoices(kit, name)}
          value={chosenVariant(kit, look, name)?.id}
          onChange={(id) => set({ ...look, variants: { ...look.variants, [name]: id } })}
        />
      ))}
    </Section>
  );
}

/** Drag to turn the character, wheel to zoom; view controls float over it. */
export function Stage() {
  const drag = useRef<{ x: number; yaw: number } | null>(null);
  const view = useStore(preview, (v) => v);
  const status = useStore(engine, (s) => s?.character.status);
  return (
    <div
      className="customizer-stage"
      onPointerDown={(e: PointerEvent) => {
        if ((e.target as Element).closest("button")) return;
        // No text selection or native drag-and-drop: either makes the browser cancel the pointer mid-turn.
        e.preventDefault();
        e.currentTarget.setPointerCapture?.(e.pointerId);
        drag.current = { x: e.clientX, yaw: preview.get().yaw };
      }}
      onPointerMove={(e: PointerEvent) => {
        if (!drag.current) return;
        preview.set({
          ...preview.get(),
          yaw: drag.current.yaw + (e.clientX - drag.current.x) * 0.01,
        });
      }}
      onPointerUp={() => {
        drag.current = null;
      }}
      onPointerCancel={() => {
        drag.current = null;
      }}
      onLostPointerCapture={() => {
        drag.current = null;
      }}
      onDragStart={(e) => e.preventDefault()}
      onWheel={(e) => {
        const current = preview.get();
        preview.set({
          ...current,
          zoom: Math.min(1, Math.max(0, current.zoom - e.deltaY * 0.0015)),
        });
      }}
    >
      {status === "loading" && (
        <p className="customizer-badge" role="status">
          Updating appearance…
        </p>
      )}
      {status === "failed" && (
        <p className="customizer-badge" role="alert">
          That look couldn’t be shown. Try another option.
        </p>
      )}
      <div className="customizer-view" role="group" aria-label="View">
        <button
          type="button"
          className="customizer-view-button"
          aria-label="Turn left"
          onClick={() => preview.set({ ...view, yaw: view.yaw - Math.PI / 4 })}
        >
          ⟲
        </button>
        <button
          type="button"
          className="customizer-view-button"
          aria-pressed={view.zoom < 0.5}
          onClick={() => preview.set({ ...view, zoom: 0 })}
        >
          Full body
        </button>
        <button
          type="button"
          className="customizer-view-button"
          aria-pressed={view.zoom >= 0.5}
          onClick={() => preview.set({ ...view, zoom: 1 })}
        >
          Face
        </button>
        <button
          type="button"
          className="customizer-view-button"
          aria-label="Turn right"
          onClick={() => preview.set({ ...view, yaw: view.yaw + Math.PI / 4 })}
        >
          ⟳
        </button>
      </div>
      <p className="customizer-hint" aria-hidden="true">
        Drag to turn · scroll to zoom
      </p>
    </div>
  );
}

export function Customizer() {
  const visible = useStore(customizerVisible, (v) => v);
  const prematch = useStore(beforeMatch, (v) => v);
  const kits = useStore(characterKits, (k) => k);
  const look = useStore(appearance, (a) => a);
  const saveError = useStore(appearanceSaveError, (e) => e);
  const [tab, setTab] = useState<Tab>("body");
  const kit = selectKit(kits, look);
  if (!visible || !kit) return null;
  const close = () => customizerVisible.set(false);
  return (
    <div className="customizer">
      <Stage />
      <aside className="customizer-panel" aria-label="Appearance" data-keyboard-owner="true">
        <header className="customizer-header">
          <div>
            <h2>Appearance</h2>
            <p className="customizer-summary">{describe(kit, look)}</p>
          </div>
          <Button
            variant="ghost"
            color="gray"
            aria-label={prematch ? "Back" : "Close appearance"}
            onClick={close}
          >
            ×
          </Button>
        </header>
        <div className="customizer-tabs" role="tablist" aria-label="Customization">
          {tabs.map(([id, label]) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              className="customizer-tab"
              onClick={() => setTab(id)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="customizer-body" role="tabpanel">
          {tab === "body" ? (
            <BodyTab kits={kits} kit={kit} look={look} set={updateAppearance} />
          ) : tab === "head" ? (
            <HeadTab kits={kits} kit={kit} look={look} set={updateAppearance} />
          ) : (
            <OutfitTab kits={kits} kit={kit} look={look} set={updateAppearance} />
          )}
        </div>
        {saveError && (
          <p className="customizer-note" role="alert">
            {saveError}
          </p>
        )}
        <footer className="customizer-footer">
          <Button
            className="action-button"
            variant="surface"
            color="gray"
            onClick={() => updateAppearance(randomAppearance(kits))}
          >
            Randomize
          </Button>
          <Button
            className="action-button"
            variant="surface"
            color="gray"
            onClick={() => updateAppearance({ ...defaultAppearance })}
          >
            Reset
          </Button>
          <span className="customizer-footer-gap" />
          {prematch ? (
            <>
              <Button variant="soft" onClick={close}>
                Save
              </Button>
              <Button onClick={() => enterArena(() => dispatch("reset"))}>Enter the arena</Button>
            </>
          ) : (
            <Button onClick={close}>Done</Button>
          )}
        </footer>
      </aside>
    </div>
  );
}
