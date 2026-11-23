import { useEffect, useMemo, useRef, useState } from 'react';

import type { RecordedReading, RecordingScope, RecordingView, RegionDef } from '../lib/bindings.ts';
import { CHECK_CHARACTERS_HINT, RECORD_RESIZED_HINT, RECORD_SCOPE_HINT, TEXT_SIZES_HINT } from '../lib/hints.ts';
import type { Size } from '../lib/region.ts';
import { background } from '../lib/repeat.ts';
import type { RecordFocus } from '../lib/useReview.ts';
import { Hint } from './Hint.tsx';
import { Question, useQuestion } from './Question.tsx';
import { PictureCanvas } from './canvas.tsx';
import { CharacterChips, MissingChips } from './CharacterChips.tsx';
import type { Discovery } from '../lib/discovery.ts';
import type { Picture } from '../lib/pixels.ts';
import {
  type ReviewEntry,
  type Untaught,
  addedShapes,
  clock,
  followingNote,
  keptNote,
  readingSize,
  reviewOrder,
  seen,
  sighted,
  sizesNote,
  tileNote,
  untaughtNote,
} from '../lib/review.ts';

/** How many of the newest readings the panel lists while recording. */
const LISTED = 6;

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

interface DiscardProps {
  /** The button's label, such as `Discard`. */
  readonly label: string;
  /** How many readings the recording holds. With none it has nothing to lose, in review either. */
  readonly readings: number;
  /** Discards the recording, and settles once it is gone or the discard failed. */
  readonly onDiscard: () => Promise<void>;
}

/** The button that discards a recording, which asks first, in its own place, when the recording has readings. */
export function DiscardRecording({ label, readings, onDiscard }: DiscardProps) {
  const [asking, setAsking] = useState(false);
  const [discarding, setDiscarding] = useState(false);
  const { control, refs } = useQuestion<HTMLButtonElement>(asking);
  if (asking) {
    return (
      <Question
        refs={refs}
        label="Discard the recording?"
        keepLabel="Keep recording"
        actLabel="Discard recording"
        busy={discarding}
        onKeep={() => {
          setAsking(false);
        }}
        onAct={() => {
          setDiscarding(true);
          background(
            onDiscard().finally(() => {
              setDiscarding(false);
              setAsking(false);
            }),
          );
        }}
      >
        Discard the recording? You lose its {plural(readings, 'reading', 'readings')}, with any text you typed and any
        readings you kept in review.
      </Question>
    );
  }
  return (
    <button
      ref={control}
      type="button"
      className="btn"
      onClick={() => {
        if (readings === 0) {
          background(onDiscard());
        } else {
          setAsking(true);
        }
      }}
    >
      {label}
    </button>
  );
}

/** What a recording of the whole window says of the `count` places the window shows text like its own in now. */
function spotsNote(count: number): string {
  return count === 0
    ? 'The window shows no text like it right now.'
    : `Text like it shows in ${plural(count, 'place', 'places')} right now, outlined on the window.`;
}

/**
 * Outlines over the live window, scaled from the window size a recording records at to `shown`: with `place`, the
 * region it records, solid, and dashed, where a recording of the whole window found text like its region's the last
 * time it looked.
 */
export function RecordingSpots({
  view,
  shown,
  place,
}: {
  readonly view: RecordingView;
  readonly shown: Size;
  readonly place: boolean;
}) {
  const { width, height } = view.at;
  const region = place ? view.place : null;
  if ((view.spots.length === 0 && region === null) || width === 0 || height === 0) {
    return null;
  }
  const x = shown.width / width;
  const y = shown.height / height;
  const at = (spot: RegionDef) => ({
    left: spot.x * x,
    top: spot.y * y,
    width: spot.width * x,
    height: spot.height * y,
  });
  return (
    <div className="record-spots" aria-hidden="true">
      {region === null ? null : <span className="place" style={at(region)} />}
      {view.spots.map((spot) => (
        <span key={`${spot.x},${spot.y},${spot.width},${spot.height}`} className="spot" style={at(spot)} />
      ))}
    </div>
  );
}

/**
 * `lead` and the sizes of text a recording of the whole window looks for, with what a size is; nothing for a recording
 * of one place.
 */
function TextSizes({ view, lead, id }: { readonly view: RecordingView; readonly lead: string; readonly id: string }) {
  const sizes = sizesNote(view.sizes);
  return sizes === null ? null : (
    <div className="field">
      <span className="name">
        {lead} {sizes}.
        <Hint topic="Text sizes" id={id}>
          {TEXT_SIZES_HINT}
        </Hint>
      </span>
    </div>
  );
}

/**
 * That a recording records at another window size than its place was drawn at, with what it does there; nothing at
 * the size it was drawn at, or while a problem says why it records nothing.
 */
function Following({ view }: { readonly view: RecordingView }) {
  const note = view.problem === null ? followingNote(view) : null;
  return note === null ? null : (
    <div className="field">
      <span className="name">
        {note}
        <Hint topic="Window size" id="hint-record-resized">
          {RECORD_RESIZED_HINT}
        </Hint>
      </span>
    </div>
  );
}

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

/**
 * The characters a recording found so far: the needed ones nothing shows yet, the ones only the text reader's
 * guesses show, and the ones the reference knows or the user checked.
 */
function Discovered({
  found,
  needs,
}: {
  readonly found: Discovery;
  /** Whether the reference reads some characters, so that some can be missing. */
  readonly needs: boolean;
}) {
  return (
    <dl className="discovered" aria-live="polite">
      {needs ? (
        <div>
          <dt>Still missing</dt>
          <dd>
            {found.missing.length === 0 ? (
              <span className="ok">None. Every character it needs has shown.</span>
            ) : (
              <MissingChips characters={found.missing} />
            )}
          </dd>
        </div>
      ) : null}
      {found.unchecked.length === 0 ? null : (
        <div>
          <dt>Found, not checked yet</dt>
          <dd>
            <span className="glyphs unchecked" role="img" aria-label={`Characters: ${found.unchecked.join(' ')}`}>
              {found.unchecked.map((character) => (
                <span key={character} className="glyph" aria-hidden="true">
                  {character}
                </span>
              ))}
            </span>
            <span className="note">
              AppyTappy read these, and they are its guess until you check their readings in review: change a
              reading&apos;s text or its Keep, or select Keep all or the button that keeps them.
            </span>
          </dd>
        </div>
      )}
      {found.found.length === 0 ? null : (
        <div>
          <dt>Found</dt>
          <dd>
            <CharacterChips characters={found.found} />
          </dd>
        </div>
      )}
      {found.unnamed === 0 ? null : (
        <div>
          <dt>Without text</dt>
          <dd className="note">
            {found.unnamed === 1
              ? '1 shape has no character yet. If it is a character, type the text of the reading that shows it.'
              : `${found.unnamed} shapes have no character yet. If they are characters, type the text of the readings that show them.`}
          </dd>
        </div>
      )}
    </dl>
  );
}

interface PanelProps {
  readonly view: RecordingView | null;
  /** The characters the recording found so far. */
  readonly found: Discovery;
  /** Whether the reference reads some characters. */
  readonly needs: boolean;
  readonly reviewing: boolean;
  /** How many readings the check learns from now. */
  readonly kept: number;
  /** How many readings review would keep. */
  readonly keeping: number;
  /** Why recording cannot start yet, or null when it can. */
  readonly blocked: string | null;
  /** Where a new recording looks for characters. */
  readonly scope: RecordingScope;
  readonly onScope: (scope: RecordingScope) => void;
  readonly appName: string;
  readonly crops: ReadonlyMap<number, Picture>;
  readonly error: string | null;
  readonly onStart: () => void;
  /** Goes on with the stopped recording. */
  readonly onMore: () => void;
  readonly onStop: () => void;
  /** Discards the recording, and settles once it is gone or the discard failed. */
  readonly onDiscard: () => Promise<void>;
  readonly onReview: () => void;
  /** Whether the draft's region differs from the stopped recording's. */
  readonly settingsChanged: boolean;
  /** Puts the stopped recording's settings back on the draft. */
  readonly onUseSettings: () => void;
  /** The part's title and Record more, which take the focus when the button that had it goes. */
  readonly focus: RecordFocus;
}

/** The inspector's part for recording a number's region. */
export function RecordPanel(props: PanelProps) {
  const { view, found, needs, reviewing, kept, keeping, blocked, appName, crops, error, settingsChanged, focus } =
    props;
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
      {view === null || view.full === null ? null : <p className="warn">{view.full}</p>}
      {view === null || view.problem === null ? null : <p className="warn">{view.problem}</p>}
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
    </>
  );
  if (view === null) {
    return (
      <section className="record" aria-labelledby="record-title">
        <h3 id="record-title" ref={focus.title} tabIndex={-1}>
          Record readings
        </h3>
        <p className="note">
          Record while you use {appName}. Each character that shows is kept once and read as text, so you only check it.
        </p>
        <div className="field">
          <span className="name">
            <span id="record-scope">Look in</span>
            <Hint topic="Look in" id="hint-record-scope">
              {RECORD_SCOPE_HINT}
            </Hint>
          </span>
          <div className="seg" role="group" aria-labelledby="record-scope" aria-describedby="hint-record-scope">
            {(['place', 'window'] as const).map((option) => (
              <button
                key={option}
                type="button"
                aria-pressed={props.scope === option}
                onClick={() => {
                  props.onScope(option);
                }}
              >
                {option === 'place' ? 'This place' : 'The whole window'}
                <small>{option === 'place' ? 'Only the text here' : 'Any text like it'}</small>
              </button>
            ))}
          </div>
        </div>
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
        <h3 id="record-title" ref={focus.title} tabIndex={-1}>
          {view.settings.scope === 'window' ? 'Recording the whole window' : 'Recording'}
        </h3>
        <p aria-live="polite">
          {plural(view.shapes.length, 'different shape', 'different shapes')} so far, in{' '}
          {plural(count, 'kept reading', 'kept readings')}.
        </p>
        <TextSizes view={view} lead="Looking for" id="hint-record-sizes" />
        <Following view={view} />
        {view.settings.scope === 'window' && view.problem === null ? (
          <p className="note">{spotsNote(view.spots.length)}</p>
        ) : null}
        {warn}
        <Discovered found={found} needs={needs} />
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
        {view.full === null ? (
          <p className="note">
            Recording goes on while you use {appName}.{' '}
            {found.missing.length > 0 ? 'Make it show what is still missing, then come' : 'Come'} back here to stop.
          </p>
        ) : null}
        <div className="row">
          <button type="button" className="btn primary" ref={stopButton} onClick={props.onStop}>
            Stop and review
          </button>
          <DiscardRecording label="Discard" readings={count} onDiscard={props.onDiscard} />
        </div>
      </section>
    );
  }
  if (reviewing) {
    return (
      <section className="record" aria-labelledby="record-title">
        <h3 id="record-title" ref={focus.title} tabIndex={-1}>
          Reviewing the recording
        </h3>
        <p className="note" aria-live="polite">
          {keptNote(count, kept, keeping)}
        </p>
      </section>
    );
  }
  return (
    <section className="record" aria-labelledby="record-title">
      <h3 id="record-title" ref={focus.title} tabIndex={-1}>
        Recorded readings
      </h3>
      <p className="note">{keptNote(count, kept, null)}</p>
      {warn}
      {count === 0 ? null : <Discovered found={found} needs={needs} />}
      {settingsChanged ? (
        <p className="note">The region changed since recording. Record more needs the recording&apos;s own.</p>
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
          <button ref={focus.more} type="button" className="btn" disabled={blocked !== null} onClick={props.onMore}>
            Record more
          </button>
        )}
        <DiscardRecording label="Discard" readings={count} onDiscard={props.onDiscard} />
      </div>
    </section>
  );
}

/** What a reading adds to the kept ones: its new characters with how often each showed, or that it adds nothing. */
function Adds({
  reading,
  entry,
  places,
  view,
}: {
  readonly reading: RecordedReading;
  readonly entry: ReviewEntry;
  /** The places in the reading of the shapes no other kept reading teaches. */
  readonly places: readonly number[];
  readonly view: RecordingView;
}) {
  const added = addedShapes(reading, entry.text, places, view.shapes);
  if (added.length === 0) {
    return <span className="meta">Adds nothing: other kept readings show its characters.</span>;
  }
  if (added.some((each) => each.character === null)) {
    return <span className="meta">Shows {plural(added.length, 'shape', 'shapes')} no kept reading has.</span>;
  }
  return (
    <span className="meta adds">
      {entry.keep ? 'Adds' : 'Would add'}
      <ul>
        {added.map((each, index) => (
          <li key={index}>
            <span className="glyphs">
              <span className="glyph">{each.character}</span>
            </span>
            {sighted(each.shape)}
          </li>
        ))}
      </ul>
    </span>
  );
}

interface SheetProps {
  readonly view: RecordingView;
  /** The characters the recording found, by what review keeps now. */
  readonly found: Discovery;
  /** The characters the reference says it needs; none when it does not say. */
  readonly needs: readonly string[];
  /** For each reading, the places in it of the shapes no other kept reading teaches. */
  readonly adds: readonly (readonly number[])[];
  readonly entries: readonly ReviewEntry[];
  /** Why the check cannot learn a kept reading, by reading index. */
  readonly problems: ReadonlyMap<number, string>;
  /** The readings whose text and another's give one shape two characters, by reading index with what to say. */
  readonly clashes: ReadonlyMap<number, string>;
  /** The characters only readings that are left out could teach, and those readings. */
  readonly untaught: Untaught;
  readonly crops: ReadonlyMap<number, Picture>;
  /** Why Keep is not available yet, or null when it is. */
  readonly blocked: string | null;
  readonly error: string | null;
  readonly onEntry: (index: number, entry: ReviewEntry) => void;
  readonly onKeepAll: (keep: boolean) => void;
  /** Opens the check of the kept readings' characters, or null when there is nothing to check. */
  readonly onCheck: (() => void) | null;
}

/** Every reading of a stopped recording, to keep or leave out and to correct. */
export function ReviewSheet(props: SheetProps) {
  const { view, found, needs, adds, entries, problems, clashes, untaught, crops, blocked, error } = props;
  const { onEntry, onKeepAll, onCheck } = props;
  // Opening review removes the button that had focus, so focus moves here.
  const head = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    head.current?.focus();
  }, []);
  // The order follows the readings' shapes alone, and working it out compares every reading with the others.
  const { readings } = view;
  const order = useMemo(() => reviewOrder(readings), [readings]);
  return (
    <>
      <div className="sheet-head">
        <div>
          <h2 ref={head} tabIndex={-1}>
            Review the recording
          </h2>
          <p>
            Check each reading&apos;s text and fix any the reader got wrong. Kept readings teach the number check its
            characters, as the boxes you draw do. A reading is kept for the characters no other kept reading shows.
          </p>
          {view.sizes.length < 2 ? null : (
            <TextSizes view={view} lead="This recording looked for" id="hint-review-sizes" />
          )}
          {view.readings.length === 0 ? null : <Discovered found={found} needs={needs.length > 0} />}
          {/* Keep is described by why it waits, or else by what keeping now would leave untaught. */}
          <p
            id={blocked === null && untaught.readings.length > 0 ? undefined : 'keep-blocked'}
            className={blocked === null ? 'note' : 'note pending'}
            aria-live="polite"
          >
            {blocked ?? ''}
          </p>
          {untaught.readings.length === 0 ? null : (
            <div className="untaught" role="status">
              <p id={blocked === null ? 'keep-blocked' : undefined}>
                {untaughtNote(untaught.characters)} To teach {untaught.characters.length === 1 ? 'it' : 'them'}, keep:
              </p>
              <div className="row">
                {untaught.readings.map((index) => {
                  const entry = entries[index];
                  const reading = view.readings[index];
                  return entry === undefined || reading === undefined ? null : (
                    <button
                      key={index}
                      type="button"
                      className="btn small"
                      onClick={() => {
                        onEntry(index, { ...entry, keep: true });
                      }}
                    >
                      Keep {entry.text.trim()}, first at {clock(reading.firstMs)}
                    </button>
                  );
                })}
              </div>
            </div>
          )}
          {error === null ? null : (
            <p className="warn" role="alert">
              {error}
            </p>
          )}
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
          {onCheck === null ? null : (
            <span className="with-hint">
              <button type="button" className="btn small" aria-describedby="hint-check-readings" onClick={onCheck}>
                Check characters
              </button>
              <Hint topic="Check characters" id="hint-check-readings">
                {CHECK_CHARACTERS_HINT}
              </Hint>
            </span>
          )}
        </div>
      </div>
      {view.readings.length === 0 ? (
        <p className="note">
          The recording kept no readings. Record more while the region shows its characters in their color.
        </p>
      ) : (
        <ul className="sheet" aria-label="Recorded readings">
          {order.map((index) => {
            const reading = view.readings[index];
            if (reading === undefined) {
              return null;
            }
            const entry = entries[index] ?? { keep: false, text: '' };
            const when = clock(reading.firstMs);
            const problem = problems.get(index);
            const clash = clashes.get(index);
            const note = tileNote(reading, entry, problem, clash, needs);
            const flagged = (problem !== undefined && entry.keep) || clash !== undefined;
            const noteId = `reading-${index}-note`;
            const size = readingSize(reading, view.sizes);
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
                  {size === null ? null : `, ${size}`}
                </span>
                <Adds reading={reading} entry={entry} places={adds[index] ?? []} view={view} />
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
