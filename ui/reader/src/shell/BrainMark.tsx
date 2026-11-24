// The mark of a paper the app has understood (design language §17.12): a brain; it pulses while the
// paper is being read. Nothing is shown for a paper whose turn has not come.

import type { Paper } from "@electropaper/bindings";
import { Icon, t } from "@electropaper/kit";
import { type JSX, Show } from "solid-js";

import { digestStatus, digests } from "../assistant/ask";
import { titleOf } from "../library";

export function BrainMark(props: { paper: Paper; onOpen: (p: Paper) => void }): JSX.Element {
  const working = (): boolean => !!digestStatus()?.running && digestStatus()?.paper === props.paper.id;
  const ready = (): boolean => digests().get(props.paper.id)?.ready ?? false;
  const label = (): string => (working() ? t("brain.working", { step: digestStatus()?.step ?? "" }) : t("brain.ready"));
  return (
    <Show when={working() || ready()}>
      <button
        type="button"
        class="brain-mark"
        classList={{ working: working() }}
        title={label()}
        aria-label={`${titleOf(props.paper)}: ${label()}`}
        onClick={(e) => {
          e.stopPropagation();
          props.onOpen(props.paper);
        }}
      >
        <Icon name="brain" size={16} />
      </button>
    </Show>
  );
}
