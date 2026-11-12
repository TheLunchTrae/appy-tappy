import { useEffect, useRef } from 'react';

import type { LookRecordingView } from '../lib/bindings.ts';
import { keptText } from '../lib/looks.ts';
import { clock } from '../lib/review.ts';

interface Props {
  readonly view: LookRecordingView | null;
  /** How many recorded screenshots the strip holds. */
  readonly recorded: number;
  /** How many of them nothing set up uses, which Remove all recorded removes. */
  readonly removable: number;
  /** Why recording cannot start yet, or null when it can. */
  readonly blocked: string | null;
  readonly appName: string;
  readonly error: string | null;
  readonly onStart: () => void;
  readonly onStop: () => void;
  readonly onRemoveAll: () => void;
}

/** The inspector's part for recording a value's region: a screenshot of each look it shows. */
export function LookPanel(props: Props) {
  const { view, recorded, removable, blocked, appName, error } = props;
  const isRecording = view?.recording === true;
  // Starting removes the Start button and stopping the Stop button, so focus
  // moves to the button or the title that takes its place rather than the page.
  const stopButton = useRef<HTMLButtonElement>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const wasRecording = useRef(false);
  useEffect(() => {
    if (isRecording) {
      stopButton.current?.focus();
    } else if (wasRecording.current) {
      title.current?.focus();
    }
    wasRecording.current = isRecording;
  }, [isRecording]);
  // A frame's problem, or why it stopped by itself, and a failed command are told apart.
  const warn = (
    <>
      {view === null || view.problem === null ? null : <p className="warn">{view.problem}</p>}
      {error === null ? null : <p className="warn">{error}</p>}
    </>
  );
  if (view?.recording === true) {
    return (
      <section className="record on" aria-labelledby="look-record-title">
        <h3 id="look-record-title" ref={title} tabIndex={-1}>
          Recording
        </h3>
        <p aria-live="polite">{keptText(view.kept.length)} kept so far.</p>
        {warn}
        <p className="note">Recording goes on while you use {appName}. Come back here to stop.</p>
        <div className="row">
          <button type="button" className="btn primary" ref={stopButton} onClick={props.onStop}>
            Stop recording
          </button>
        </div>
      </section>
    );
  }
  return (
    <section className="record" aria-labelledby="look-record-title">
      <h3 id="look-record-title" ref={title} tabIndex={-1}>
        Record screenshots
      </h3>
      <p className="note">
        Record the region while you use {appName}. A screenshot is kept each time the region shows a look that no
        screenshot shows yet.
      </p>
      {view === null || view.problem !== null ? null : (
        <p className="note" aria-live="polite">
          The last recording kept {keptText(view.kept.length)} in {clock(view.elapsedMs)}.
        </p>
      )}
      {warn}
      <div className="row">
        <button type="button" className="btn" disabled={blocked !== null} onClick={props.onStart}>
          Start recording
        </button>
        {recorded === 0 ? null : (
          <button type="button" className="btn" disabled={removable === 0} onClick={props.onRemoveAll}>
            Remove all recorded
          </button>
        )}
      </div>
      {blocked === null ? null : <p className="note">{blocked}</p>}
      {recorded > removable ? (
        <p className="note">A recorded screenshot stays while the region is drawn on it or a state is taken from it.</p>
      ) : null}
    </section>
  );
}
