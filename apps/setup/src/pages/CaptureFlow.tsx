import { useCallback, useEffect, useRef, useState } from 'react';

import type { AppSummary } from '../lib/bindings.ts';
import { errorText, restoreDraftScreenshots } from '../lib/api.ts';
import { useCapture } from '../lib/capture.ts';
import { useLanding } from '../lib/landing.ts';
import { ask } from '../lib/request.ts';
import { AntiCheatRunning } from '../components/AntiCheatRunning.tsx';
import { Capture, type CaptureMode, type Restored } from '../components/Capture.tsx';
import { WindowAbsent } from '../components/capture/WindowAbsent.tsx';
import { MatchEdit } from '../components/MatchEdit.tsx';
import { Picker } from '../components/Picker.tsx';
import { Trail } from '../components/Trail.tsx';
import type { Made, RefFlow, ValueFlow } from '../lib/flow.ts';
import type { Held } from '../lib/unsaved.ts';
import { useWindowSearch } from '../lib/useWindowSearch.ts';
import { windowLanding } from '../lib/windowSearch.ts';
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

/** The heading of `flow`'s page. */
function flowHeading(flow: ValueFlow | RefFlow): string {
  if (flow.kind === 'reference') {
    return flow.existing === null ? 'New reference' : `Edit ${flow.existing.reference.name}`;
  }
  const { changing } = flow;
  if (changing === null) {
    return 'New value';
  }
  return changing.how === 'fix' ? `Fix ${changing.value.name} at this size` : `Edit ${changing.value.name}`;
}

/** Where a draft's screenshots stand: none to bring back, waiting for the window, brought back, or refused. */
type Restoring =
  | { readonly kind: 'waiting' }
  | { readonly kind: 'restored'; readonly restored: Restored | null }
  | { readonly kind: 'failed'; readonly error: string };

/**
 * The capture screen of a value or reference of the saved application `app`, which finds that application's window
 * and captures it itself. A draft opens once its window is captured, since its screenshots come back only with a
 * window.
 */
function SearchedCapture({
  flow,
  app,
  mode,
  trail,
  continued,
  onCancel,
  onSaved,
  onDrafted,
  onMakeReference,
  onEditApplication,
  draftedApps,
  onHeld,
  onDraft,
}: Props & { readonly app: string; readonly mode: CaptureMode }) {
  const draft = flow.resumed;
  // The flow holds the window's capture, which would otherwise keep copying frames nobody looks at. A window that
  // closed and opened again is picked in place of the capture that ended, so the editor keeps its screenshots.
  const { pickAgain, stop } = useCapture();
  const { absence, found, antiCheat, fallback, windowFailed, refresh, choose, restart, several } = useWindowSearch(
    app,
    pickAgain,
  );
  // Each start over is a new editor, as each pick of a window is.
  const [startedOver, setStartedOver] = useState(0);
  const [restoring, setRestoring] = useState<Restoring>(
    draft === null ? { kind: 'restored', restored: null } : { kind: 'waiting' },
  );
  const captured = absence === null;
  const waitingDraft = restoring.kind === 'waiting' && draft !== null ? draft.id : null;
  // Capturing the window forgets the session's screenshots, so a draft's come back after its first capture.
  useEffect(
    () =>
      waitingDraft === null || !captured
        ? undefined
        : ask(
            async () => restoreDraftScreenshots(waitingDraft),
            (screenshots) => {
              setRestoring({ kind: 'restored', restored: { screenshots } });
            },
            (error) => {
              setRestoring({ kind: 'failed', error: errorText(error) });
            },
          ),
    [waitingDraft, captured],
  );
  const [held, setHeld] = useState<Held>('nothing');
  const reportHeld = useCallback(
    (now: Held): void => {
      setHeld(now);
      onHeld(now);
    },
    [onHeld],
  );
  const page = useRef<HTMLElement>(null);
  const holding = restoring.kind !== 'restored';
  useLanding(continued && holding, windowLanding(absence), page);
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
  const panel =
    absence === null ? null : (
      <WindowAbsent
        absence={absence}
        onRefresh={refresh}
        onChoose={choose}
        matchEdit={absence.offersMatchEdit ? matchEdit : null}
      />
    );
  if (restoring.kind !== 'restored') {
    return (
      <main className="page" ref={page}>
        <Trail trail={trail} here={flowHeading(flow)} />
        <div className="page-head">
          <h1>{flowHeading(flow)}</h1>
        </div>
        <AntiCheatRunning products={antiCheat} setting={found?.summary.file?.antiCheat ?? null} />
        {panel ??
          (restoring.kind === 'failed' ? (
            <div className="row">
              <p className="warn" role="alert">
                Couldn't bring back the draft's screenshots: {restoring.error}
              </p>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  setRestoring({ kind: 'waiting' });
                }}
              >
                Try again
              </button>
            </div>
          ) : (
            <p className="note" role="status">
              Bringing back the draft's screenshots…
            </p>
          ))}
        <div className="actions">
          <button type="button" className="btn" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </main>
    );
  }
  return (
    <Capture
      key={startedOver}
      choice={{ app, name: found?.name ?? 'the application' }}
      absent={absence === null || panel === null ? null : { reason: absence.reason, panel }}
      windowNotes={
        <>
          <AntiCheatRunning products={antiCheat} setting={found?.summary.file?.antiCheat ?? null} />
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
      resumed={draft}
      restored={restoring.restored}
      trail={trail}
      onCancel={onCancel}
      onChangeWindow={
        several
          ? () => {
              stop();
              restart();
              // The new window starts over, without what the draft kept.
              if (draft !== null) {
                setRestoring({ kind: 'restored', restored: { screenshots: null } });
              }
              setStartedOver((count) => count + 1);
            }
          : null
      }
      onSaved={onSaved}
      onDrafted={onDrafted}
      onMakeReference={onMakeReference}
      onHeld={reportHeld}
      onDraft={onDraft}
    />
  );
}

/**
 * Making a value, fixing a saved one at another window size, editing one, or making or changing a reference. A
 * value or reference whose application is known, its own, its draft's, or one handed over, opens its capture screen
 * at once, which finds and captures that application's window itself. Anything else first chooses its application.
 */
export function CaptureFlow(props: Props) {
  // What the flow started for: its mode stays the same object while it shows.
  const [{ mode, lockedApp }] = useState(() => subject(props.flow));
  const [app, setApp] = useState(() => lockedApp ?? props.flow.resumed?.def.app ?? props.flow.app);
  const { onAddApplication } = props;
  const addApplication = useCallback((): void => {
    onAddApplication(null);
  }, [onAddApplication]);
  return app === null ? (
    <Picker
      heading={flowHeading(props.flow)}
      trail={props.trail}
      continued={props.continued}
      onCancel={props.onCancel}
      onChoose={setApp}
      onAddApplication={addApplication}
    />
  ) : (
    <SearchedCapture {...props} app={app} mode={mode} />
  );
}
