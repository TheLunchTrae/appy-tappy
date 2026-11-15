import { useCallback, useState } from 'react';

import type { AppSummary } from '../lib/bindings.ts';
import { restoreDraftScreenshots } from '../lib/api.ts';
import { useCapture } from '../lib/capture.ts';
import { AntiCheatRunning } from '../components/AntiCheatRunning.tsx';
import { Capture, type CaptureMode, type Restored } from '../components/Capture.tsx';
import { WindowAbsent } from '../components/capture/WindowAbsent.tsx';
import { MatchEdit } from '../components/MatchEdit.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import type { Changing, Made, RefFlow, ValueFlow } from '../lib/flow.ts';
import type { Held } from '../lib/unsaved.ts';
import { useWindowSearch } from '../lib/useWindowSearch.ts';
import type { ReportDraft } from '../lib/drafts.ts';

interface Props {
  /**
   * What the flow makes or changes: a new value, a saved value to fix or edit, or a reference. Its application is
   * the one a new value or reference starts with, such as one just added for it, and its draft the one it finishes.
   */
  readonly flow: ValueFlow | RefFlow;
  /** The way to this flow, for its breadcrumb. */
  readonly trail: readonly string[];
  /** Whether the flow shows because another flow opened it or ended, so the focus has to land somewhere. */
  readonly continued: boolean;
  readonly onCancel: () => void;
  /** Hears where the value was saved, or the reference's application. */
  readonly onSaved: (made: Made) => void;
  /** Closes the flow once its draft is saved or deleted. */
  readonly onDrafted: () => void;
  /** Leaves to add an application, and comes back with it chosen, or with `chosen` when that is canceled. */
  readonly onAddApplication: (chosen: string | null) => void;
  /** Leaves to change how `app` is found, and comes back to it. */
  readonly onEditApplication: (app: AppSummary) => void;
  /** The saved applications that have a draft of changes. */
  readonly draftedApps: ReadonlySet<string>;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
  /** Hears how to save the flow's draft. */
  readonly onDraft: ReportDraft;
}

/** The picker's heading for each way a saved value changes. */
const HEADINGS: Readonly<Record<Changing['how'], (name: string) => string>> = {
  fix: (name) => `Fix ${name} at this size`,
  edit: (name) => `Edit ${name}`,
};

/** How the capture runs for `flow`, the saved application it is locked to, and the picker's heading. */
function subject(flow: ValueFlow | RefFlow): {
  readonly mode: CaptureMode;
  readonly lockedApp: string | null;
  readonly heading: string;
} {
  if (flow.kind === 'reference') {
    const { existing } = flow;
    return {
      mode: { kind: 'reference', existing: existing?.reference ?? null },
      lockedApp: existing?.app ?? null,
      heading: existing === null ? 'New reference' : `Edit ${existing.reference.name}`,
    };
  }
  const { changing } = flow;
  return changing === null
    ? { mode: { kind: 'value' }, lockedApp: null, heading: 'New value' }
    : {
        mode: { kind: changing.how, value: changing.value },
        lockedApp: changing.app,
        heading: HEADINGS[changing.how](changing.value.name),
      };
}

/** The editor of a saved value or reference, which shows with or without its application's window. */
function SavedCapture({
  app,
  mode,
  trail,
  onCancel,
  onSaved,
  onDrafted,
  onEditApplication,
  draftedApps,
  onHeld,
  onDraft,
}: Props & { readonly app: string; readonly mode: CaptureMode }) {
  // The flow holds the window's capture, which would otherwise keep copying frames nobody looks at.
  const { pick, stop } = useCapture();
  const { absence, found, antiCheat, refresh, choose, restart } = useWindowSearch(app, pick);
  // Each start over is a new editor, as each pick of a window is.
  const [startedOver, setStartedOver] = useState(0);
  const [held, setHeld] = useState<Held>('nothing');
  const reportHeld = useCallback(
    (now: Held): void => {
      setHeld(now);
      onHeld(now);
    },
    [onHeld],
  );
  return (
    <Capture
      key={startedOver}
      choice={{ app, name: found?.name ?? 'the application' }}
      absent={
        absence === null
          ? null
          : {
              reason: absence.reason,
              panel: (
                <WindowAbsent
                  absence={absence}
                  onRefresh={refresh}
                  onChoose={choose}
                  matchEdit={
                    found !== null && absence.offersMatchEdit ? (
                      <MatchEdit
                        app={found}
                        drafted={draftedApps.has(app)}
                        disabled={absence.busy}
                        blocked={
                          held === 'nothing'
                            ? null
                            : "Save or discard what you changed here first: changing how it's found leaves this editor."
                        }
                        onEdit={() => {
                          onEditApplication(found.summary);
                        }}
                      />
                    ) : null
                  }
                />
              ),
            }
      }
      windowNotes={<AntiCheatRunning products={antiCheat} />}
      mode={mode}
      resumed={null}
      restored={null}
      trail={trail}
      onCancel={onCancel}
      onChangeWindow={() => {
        stop();
        restart();
        setStartedOver((count) => count + 1);
      }}
      onSaved={onSaved}
      onDrafted={onDrafted}
      onHeld={reportHeld}
      onDraft={onDraft}
    />
  );
}

/** The picker, then the capture of the window it picked. */
function PickedCapture({
  flow,
  trail,
  continued,
  onCancel,
  onSaved,
  onDrafted,
  onAddApplication,
  onEditApplication,
  draftedApps,
  onHeld,
  onDraft,
  lockedApp,
  mode,
  heading,
}: Props & { readonly lockedApp: string | null; readonly mode: CaptureMode; readonly heading: string }) {
  const draft = flow.resumed;
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
  const [restored, setRestored] = useState<Restored | null>(null);
  // Whether a window was picked before: only the first pick brings a draft's screenshots back.
  const [pickedBefore, setPickedBefore] = useState(false);
  // The flow holds the window's capture, which would otherwise keep copying frames nobody looks at.
  const { pick, stop } = useCapture();
  if (picking || choice === null) {
    return (
      <Picker
        heading={heading}
        trail={trail}
        continued={continued}
        lockedApp={lockedApp ?? draft?.def.app ?? null}
        startApp={choice?.app ?? flow.app}
        startWindow={choice?.window.handle ?? null}
        onCancel={onCancel}
        onAddApplication={onAddApplication}
        onEditApplication={onEditApplication}
        draftedApps={draftedApps}
        onCapture={pick}
        onPick={async (picked) => {
          // Picking the window forgets the session's screenshots, so a draft's come back after it, the first time.
          if (draft !== null) {
            setRestored({ screenshots: pickedBefore ? null : await restoreDraftScreenshots(draft.id) });
          }
          setPickedBefore(true);
          setChoice(picked);
          setPicking(false);
        }}
      />
    );
  }
  return (
    <Capture
      choice={choice}
      absent={null}
      windowNotes={null}
      mode={mode}
      resumed={draft}
      restored={restored}
      trail={trail}
      onCancel={onCancel}
      onChangeWindow={() => {
        // The capture ends here, not when another window is picked, so nothing keeps its screenshots meanwhile.
        stop();
        setPicking(true);
      }}
      onSaved={onSaved}
      onDrafted={onDrafted}
      onHeld={onHeld}
      onDraft={onDraft}
    />
  );
}

/**
 * Making a value, fixing a saved one at another window size, editing one, or making or changing a reference. A
 * saved value or reference opens in its editor at once, which looks for its application's window itself. Anything
 * new, and a draft, whose screenshots come back only with a window, chooses the application and captures its window
 * first.
 */
export function CaptureFlow(props: Props) {
  // What the flow started for: its mode stays the same object while it shows.
  const [{ mode, lockedApp, heading }] = useState(() => subject(props.flow));
  return lockedApp !== null && props.flow.resumed === null ? (
    <SavedCapture {...props} app={lockedApp} mode={mode} />
  ) : (
    <PickedCapture {...props} lockedApp={lockedApp} mode={mode} heading={heading} />
  );
}
