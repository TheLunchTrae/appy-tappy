import { useCallback, useReducer, useState } from 'react';

import type { AppSummary } from '../lib/bindings.ts';
import { captureEnded, restoreDraftScreenshots } from '../lib/api.ts';
import { background, backgroundLater } from '../lib/repeat.ts';
import { useCapture } from '../lib/capture.ts';
import { AntiCheatRunning } from '../components/AntiCheatRunning.tsx';
import { Capture, type CaptureMode, type Restored } from '../components/Capture.tsx';
import { WindowAbsent } from '../components/capture/WindowAbsent.tsx';
import { MatchEdit } from '../components/MatchEdit.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import type { Made, RefFlow, ValueFlow } from '../lib/flow.ts';
import type { Held } from '../lib/unsaved.ts';
import { useWindowSearch } from '../lib/useWindowSearch.ts';
import { ENDED_ASK_PAUSE_MS, FIRST_PICK, endedAsks, pickedWindowStep, windowLanding } from '../lib/windowSearch.ts';
import type { ReportDraft, Resumed } from '../lib/drafts.ts';

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
  /**
   * Closes a new value's flow once it is kept as the draft `value`, and opens `reference`, the draft of a reference
   * for it; null where **New reference** is not offered.
   */
  readonly onMakeReference: ((reference: Resumed, value: Resumed) => void) | null;
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

/** How the capture runs for `flow`, and the saved application it is locked to. */
function subject(flow: ValueFlow | RefFlow): { readonly mode: CaptureMode; readonly lockedApp: string | null } {
  if (flow.kind === 'reference') {
    const { existing } = flow;
    return { mode: { kind: 'reference', existing: existing?.reference ?? null }, lockedApp: existing?.app ?? null };
  }
  const { changing } = flow;
  return changing === null
    ? { mode: { kind: 'value' }, lockedApp: null }
    : { mode: { kind: changing.how, value: changing.value }, lockedApp: changing.app };
}

/** The picker's heading for `flow`. */
function pickerHeading(flow: ValueFlow | RefFlow): string {
  if (flow.kind === 'reference') {
    return flow.existing === null ? 'New reference' : `Edit ${flow.existing.reference.name}`;
  }
  const { changing } = flow;
  if (changing === null) {
    return 'New value';
  }
  return changing.how === 'fix' ? `Fix ${changing.value.name} at this size` : `Edit ${changing.value.name}`;
}

const NO_LANDING = { owed: false, target: null };

/** The editor of a saved value or reference, which shows with or without its application's window. */
function SavedCapture({
  app,
  mode,
  trail,
  continued,
  onCancel,
  onSaved,
  onDrafted,
  onEditApplication,
  draftedApps,
  onHeld,
  onDraft,
}: Props & { readonly app: string; readonly mode: CaptureMode }) {
  // The flow holds the window's capture, which would otherwise keep copying frames nobody looks at. A window that
  // closed and opened again is picked in place of the capture that ended, so the editor keeps its screenshots.
  const { pickAgain, stop } = useCapture();
  const { absence, found, antiCheat, fallback, windowFailed, refresh, choose, restart } = useWindowSearch(
    app,
    pickAgain,
  );
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
  const matchEdit =
    found === null ? null : (
      <MatchEdit
        app={found}
        drafted={draftedApps.has(app)}
        disabled={absence?.busy ?? false}
        blocked={
          held === 'nothing'
            ? null
            : "Save or discard what you changed here first: changing how it's found leaves this editor."
        }
        onEdit={() => {
          onEditApplication(found.summary);
        }}
      />
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
                  matchEdit={absence.offersMatchEdit ? matchEdit : null}
                />
              ),
            }
      }
      windowNotes={
        <>
          <AntiCheatRunning products={antiCheat} />
          {fallback === null ? null : (
            <>
              <p className="warn">{fallback}</p>
              {matchEdit}
            </>
          )}
        </>
      }
      onWindowFailed={windowFailed}
      landing={{ owed: continued, target: windowLanding(absence) }}
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
      onMakeReference={null}
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
  onMakeReference,
  onAddApplication,
  onEditApplication,
  draftedApps,
  onHeld,
  onDraft,
  lockedApp,
  mode,
}: Props & { readonly lockedApp: string | null; readonly mode: CaptureMode }) {
  const draft = flow.resumed;
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
  const [restored, setRestored] = useState<Restored | null>(null);
  // Whether a window was picked before: only the first pick brings a draft's screenshots back.
  const [pickedBefore, setPickedBefore] = useState(false);
  // The flow holds the window's capture, which would otherwise keep copying frames nobody looks at.
  const { pick, stop } = useCapture();
  // Whether the picked window closed, which only another pick gets past.
  const [pickedWindow, tell] = useReducer(pickedWindowStep, FIRST_PICK);
  const [askEnded] = useState(() => endedAsks(captureEnded, ENDED_ASK_PAUSE_MS, Date.now, backgroundLater));
  const asked = pickedWindow.pick;
  const windowFailed = useCallback((): void => {
    background(
      askEnded(() => {
        tell({ kind: 'ended', pick: asked });
      }),
    );
  }, [askEnded, asked]);
  if (picking || choice === null) {
    return (
      <Picker
        heading={pickerHeading(flow)}
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
      windowNotes={
        pickedWindow.closed ? <p className="warn">The window closed. Select Change window to pick it again.</p> : null
      }
      onWindowFailed={windowFailed}
      landing={NO_LANDING}
      mode={mode}
      resumed={draft}
      restored={restored}
      trail={trail}
      onCancel={onCancel}
      onChangeWindow={() => {
        // The capture ends here, not when another window is picked, so nothing keeps its screenshots meanwhile.
        stop();
        tell({ kind: 'repicked' });
        setPicking(true);
      }}
      onSaved={onSaved}
      onDrafted={onDrafted}
      onMakeReference={onMakeReference}
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
  const [{ mode, lockedApp }] = useState(() => subject(props.flow));
  return lockedApp !== null && props.flow.resumed === null ? (
    <SavedCapture {...props} app={lockedApp} mode={mode} />
  ) : (
    <PickedCapture {...props} lockedApp={lockedApp} mode={mode} />
  );
}
