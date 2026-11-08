import { useEffect, useRef } from 'react';

import type { RecordedReading, RecordingView } from '../lib/bindings.ts';
import { PictureCanvas } from './canvas.tsx';
import type { Picture } from '../lib/pixels.ts';
import { type ReviewEntry, clock, seen, tileNote } from '../lib/review.ts';

/** How many of the newest readings the panel lists while recording. */
const LISTED = 6;

/** A recorded reading's pixels, or an empty box until they arrive. */
function Crop({ picture, label }: { readonly picture: Picture | undefined; readonly label: string }) {
  return (
    <span className="crop">{picture === undefined ? null : <PictureCanvas picture={picture} label={label} />}</span>
  );
}

function guess(reading: RecordedReading): { readonly text: string; readonly pending: boolean } {
  switch (reading.text.kind) {
    case 'pending':
      return { text: 'Reading…', pending: true };
    case 'failed':
      return { text: 'Type it in review', pending: true };
    case 'read':
      return reading.text.text === ''
        ? { text: 'No text found', pending: true }
        : { text: reading.text.text, pending: false };
  }
}

interface PanelProps {
  readonly view: RecordingView | null;
  readonly reviewing: boolean;
  /** How many readings the check learns from now. */
  readonly kept: number;
  /** How many readings review would keep. */
  readonly keeping: number;
  /** Why recording cannot start yet, or null when it can. */
  readonly blocked: string | null;
  readonly appName: string;
  readonly crops: ReadonlyMap<number, Picture>;
  readonly error: string | null;
  readonly onStart: () => void;
  /** Goes on with the stopped recording. */
  readonly onMore: () => void;
  readonly onStop: () => void;
  readonly onDiscard: () => void;
  readonly onReview: () => void;
  /** Whether the draft's region, color or tolerance differ from the stopped recording's. */
  readonly settingsChanged: boolean;
  /** Puts the stopped recording's settings back on the draft. */
  readonly onUseSettings: () => void;
}

/** The inspector's part for recording a number's region. */
export function RecordPanel(props: PanelProps) {
  const { view, reviewing, kept, keeping, blocked, appName, crops, error, settingsChanged } = props;
  const isRecording = view?.recording === true;
  // Starting removes the Start button, so focus moves to Stop rather than the page.
  const stopButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (isRecording) {
      stopButton.current?.focus();
    }
  }, [isRecording]);
  // A frame's problem and a failed command are told apart, and a later
  // command's success clears only its own error.
  const warn = (
    <>
      {view === null || view.problem === null ? null : <p className="warn">{view.problem}</p>}
      {error === null ? null : <p className="warn">{error}</p>}
    </>
  );
  if (view === null) {
    return (
      <section className="record" aria-labelledby="record-title">
        <h3 id="record-title" tabIndex={-1}>
          Record readings
        </h3>
        <p className="note">
          Record the region while you use {appName}. Each different reading is kept once and read as text, so you only
          check it.
        </p>
        {warn}
        <div className="row">
          <button type="button" className="btn" disabled={blocked !== null} onClick={props.onStart}>
            Start recording
          </button>
        </div>
        {blocked === null ? null : <p className="note">{blocked}</p>}
      </section>
    );
  }
  const count = view.readings.length;
  if (view.recording) {
    const newest = view.readings
      .map((reading, index) => ({ reading, index }))
      .slice(-LISTED)
      .toReversed();
    return (
      <section className="record on" aria-labelledby="record-title">
        <h3 id="record-title" tabIndex={-1}>
          Recording
        </h3>
        <p aria-live="polite">
          {count === 1 ? '1 different reading so far.' : `${count} different readings so far.`}
          {view.full ? ' That is as many as one recording keeps.' : ''}
        </p>
        {warn}
        {count === 0 ? null : (
          <ul className="found">
            {newest.map(({ reading, index }) => {
              const shown = guess(reading);
              return (
                <li key={index}>
                  <Crop picture={crops.get(index)} label={`Reading at ${clock(reading.firstMs)}`} />
                  <span className={shown.pending ? 'guess pending' : 'guess'}>{shown.text}</span>
                  <span className="when">{clock(reading.firstMs)}</span>
                </li>
              );
            })}
          </ul>
        )}
        <p className="note">Recording goes on while you use {appName}. Come back here to stop.</p>
        <div className="row">
          <button type="button" className="btn primary" ref={stopButton} onClick={props.onStop}>
            Stop and review
          </button>
          <button type="button" className="btn" onClick={props.onDiscard}>
            Discard
          </button>
        </div>
      </section>
    );
  }
  if (reviewing) {
    return (
      <section className="record" aria-labelledby="record-title">
        <h3 id="record-title" tabIndex={-1}>
          Reviewing the recording
        </h3>
        <p className="note" aria-live="polite">
          {keeping} of {count} readings will be kept. The value saves only what you keep.
        </p>
      </section>
    );
  }
  return (
    <section className="record" aria-labelledby="record-title">
      <h3 id="record-title" tabIndex={-1}>
        Recorded readings
      </h3>
      <p className="note">
        {kept === 0
          ? `${count === 1 ? '1 reading' : `${count} readings`} recorded, none kept yet.`
          : `Learned from ${kept === 1 ? '1 recorded reading' : `${kept} recorded readings`}.`}
      </p>
      {warn}
      {settingsChanged ? (
        <p className="note">
          The region, color or tolerance changed since recording. Record more needs the recording&apos;s own.
        </p>
      ) : null}
      <div className="row">
        <button type="button" className="btn" onClick={props.onReview}>
          Review recording
        </button>
        {settingsChanged ? (
          <button type="button" className="btn" onClick={props.onUseSettings}>
            Use its settings
          </button>
        ) : (
          <button type="button" className="btn" data-record-more disabled={blocked !== null} onClick={props.onMore}>
            Record more
          </button>
        )}
        <button type="button" className="btn" onClick={props.onDiscard}>
          Discard
        </button>
      </div>
    </section>
  );
}

interface SheetProps {
  readonly view: RecordingView;
  readonly entries: readonly ReviewEntry[];
  /** Why the check cannot learn a kept reading, by reading index. */
  readonly problems: ReadonlyMap<number, string>;
  readonly crops: ReadonlyMap<number, Picture>;
  /** Why Keep is not available yet, or null when it is. */
  readonly blocked: string | null;
  readonly error: string | null;
  readonly onEntry: (index: number, entry: ReviewEntry) => void;
  readonly onKeepAll: (keep: boolean) => void;
}

/** Every reading of a stopped recording, to keep or leave out and to correct. */
export function ReviewSheet(props: SheetProps) {
  const { view, entries, problems, crops, blocked, error, onEntry, onKeepAll } = props;
  // Opening review removes the button that had focus, so focus moves here.
  const head = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    head.current?.focus();
  }, []);
  return (
    <>
      <div className="sheet-head">
        <div>
          <h2 ref={head} tabIndex={-1}>
            Review the recording
          </h2>
          <p>
            Check each reading&apos;s text and fix any the reader got wrong. Kept readings teach the number check its
            characters, like labeled screenshots.
          </p>
          <p id="keep-blocked" className={blocked === null ? 'note' : 'note pending'} aria-live="polite">
            {blocked ?? ''}
          </p>
          {error === null ? null : <p className="warn">{error}</p>}
        </div>
        <div className="end">
          <button
            type="button"
            className="btn small"
            onClick={() => {
              onKeepAll(true);
            }}
          >
            Keep all
          </button>
          <button
            type="button"
            className="btn small"
            onClick={() => {
              onKeepAll(false);
            }}
          >
            Keep none
          </button>
        </div>
      </div>
      {view.readings.length === 0 ? (
        <p className="note">
          The recording kept no readings. Record more while the region shows its characters in their color.
        </p>
      ) : (
        <ul className="sheet" aria-label="Recorded readings">
          {view.readings.map((reading, index) => {
            const entry = entries[index] ?? { keep: false, text: '' };
            const when = clock(reading.firstMs);
            const problem = problems.get(index);
            const note = tileNote(reading, entry, problem);
            const flagged = problem !== undefined && entry.keep;
            const noteId = `reading-${index}-note`;
            return (
              <li key={index} className={entry.keep ? 'tile kept' : 'tile off'}>
                <label className="top">
                  <input
                    type="checkbox"
                    aria-label={`Keep the reading at ${when}`}
                    checked={entry.keep}
                    onChange={(event) => {
                      onEntry(index, { ...entry, keep: event.target.checked });
                    }}
                  />
                  Keep
                </label>
                <Crop picture={crops.get(index)} label={`Reading at ${when}`} />
                <input
                  className={flagged ? 'input bad' : 'input'}
                  placeholder="Type what it shows"
                  aria-label={`Text of the reading at ${when}`}
                  aria-invalid={flagged}
                  aria-describedby={note === null ? undefined : noteId}
                  value={entry.text}
                  onChange={(event) => {
                    const text = event.target.value;
                    // A reading the reader found no text on is kept once the user types its text.
                    const typed = entry.text.trim() === '' && text.trim() !== '';
                    onEntry(index, { keep: entry.keep || typed, text });
                  }}
                />
                <span className="meta">
                  First at {when}, {seen(reading.seen)}
                </span>
                {note === null ? null : (
                  <span id={noteId} className={flagged ? 'flag' : 'meta'}>
                    {note}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}
