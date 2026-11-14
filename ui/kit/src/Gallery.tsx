// The kit gallery (P1.MD.KIT.3): every component in every state, for review in all themes.
// Opened in the Reader with `#gallery` at the end of its address (development and e2e only).

import { For, type JSX, createSignal } from "solid-js";

import { Banner, Button, Card, Dialog, EmptyState, IconButton, Kbd, Segmented, Skeleton, StatusItem, StatusLine, Swatches, openMenu, showToast } from "./components";
import { ICONS, type IconName } from "./icons";
import { THEMES, type Theme, setTheme, theme } from "./theme";

const COLORS = ["#ffd400", "#7bd88f", "#74c0fc", "#ff8787", "#d0a2f7"];

function Section(props: { title: string; children: JSX.Element }): JSX.Element {
  return (
    <section class="gallery-section">
      <h2>{props.title}</h2>
      <div class="gallery-row">{props.children}</div>
    </section>
  );
}

export function Gallery(): JSX.Element {
  const [seg, setSeg] = createSignal<"a" | "b" | "c">("a");
  const [color, setColor] = createSignal(COLORS[0] ?? "");
  const [pressed, setPressed] = createSignal(true);
  const [dialog, setDialog] = createSignal(false);
  return (
    <div class="gallery">
      <header class="gallery-head">
        <strong>Kit gallery</strong>
        <Segmented label="Theme" value={theme()} options={THEMES.map((th) => ({ value: th, label: th }))} onChange={(v: Theme) => setTheme(v)} />
      </header>
      <Section title="Buttons">
        <Button variant="primary">Save into PDF</Button>
        <Button>Cancel</Button>
        <Button variant="quiet" icon="plus">
          Add PDF
        </Button>
        <Button variant="danger" icon="trash">
          Delete
        </Button>
        <Button disabled>Disabled</Button>
      </Section>
      <Section title="Icon buttons">
        <IconButton icon="highlighter" label="Highlight" shortcut="H" pressed={pressed()} swatch={color()} onClick={() => setPressed(!pressed())} />
        <IconButton icon="file-down" label="Save into PDF" shortcut="Ctrl+S" dot />
        <IconButton icon="undo-2" label="Undo" shortcut="Ctrl+Z" />
        <IconButton icon="redo-2" label="Redo" shortcut="Ctrl+Y" disabled />
        <IconButton icon="x" label="Close" size="sm" />
        <IconButton icon="ellipsis" label="More" onClick={(e) => openMenu("More", [{ heading: "View" }, { label: "Focus mode", icon: "maximize-2", shortcut: "F11", run: () => {} }, { label: "Compact", checked: true, run: () => {} }, { separator: true }, { label: "Delete", icon: "trash", danger: true, run: () => {} }], e.currentTarget.getBoundingClientRect())} />
      </Section>
      <Section title="Choices">
        <Segmented label="Layout" value={seg()} options={[{ value: "a", label: "Continuous" }, { value: "b", label: "Single" }, { value: "c", label: "Two pages" }]} onChange={setSeg} />
        <Swatches label="Colour" colors={COLORS} value={color()} onPick={setColor} />
        <Kbd>Ctrl+K</Kbd>
        <input class="ep-input" placeholder="Search notes" />
        <select class="ep-select">
          <option>Definition</option>
        </select>
      </Section>
      <Section title="Card">
        <Card title="Highlight" icon="highlighter" meta="Taha · 14 Nov 2026" onClose={() => {}} footer={<Button variant="danger" icon="trash">Delete</Button>}>
          <blockquote class="gallery-quote">the boundary layer separates near the trailing edge</blockquote>
          <textarea class="ep-input" rows={2} placeholder="Add a comment" />
        </Card>
      </Section>
      <Section title="Empty, loading, problem">
        <div class="gallery-box">
          <EmptyState icon="notebook-pen" text="Your notes on this paper. Select text and choose Add to notes, or start typing." action={<Button>Start a note</Button>} />
        </div>
        <div class="gallery-box">
          <Skeleton lines={5} />
        </div>
        <div class="gallery-box">
          <Banner onClose={() => {}}>The library folder cannot be opened. Choose another folder.</Banner>
        </div>
      </Section>
      <Section title="Overlays">
        <Button onClick={() => showToast("Copied with citation", { label: "Undo", run: () => {} })}>Show a toast</Button>
        <Button onClick={() => setDialog(true)}>Open a dialog</Button>
      </Section>
      <Section title="Icons">
        <For each={Object.keys(ICONS) as IconName[]}>{(n) => <IconButton icon={n} label={n} />}</For>
      </Section>
      <StatusLine
        left={
          <>
            <StatusItem icon="check">Saved</StatusItem>
            <StatusItem icon="file-down" tone="accent">
              Changes not in PDF
            </StatusItem>
          </>
        }
        right={<StatusItem>Page 3 of 14 · 110 %</StatusItem>}
      />
      {dialog() ? (
        <Dialog title="Delete the collection “Fluids”?" onCancel={() => setDialog(false)} actions={<><Button onClick={() => setDialog(false)}>Cancel</Button><Button variant="danger" onClick={() => setDialog(false)}>Delete</Button></>}>
          The papers stay in your library.
        </Dialog>
      ) : null}
    </div>
  );
}
