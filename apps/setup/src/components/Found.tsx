import { useEffect, useRef } from 'react';

import { type FoundRow, keptState, listed } from '../lib/found.ts';
import { CHECK_CHARACTERS_HINT, FIND_LIKE_HINT, FOUND_HINT, REREAD_HINT } from '../lib/hints.ts';
import type { Picture } from '../lib/pixels.ts';
import { background } from '../lib/repeat.ts';
import type { Finder } from '../lib/useFound.ts';
import type { Size } from '../lib/region.ts';
import { PictureCanvas } from './canvas.tsx';
import { Hint } from './Hint.tsx';

interface PanelProps {
  readonly finder: Finder;
  /** Why the search cannot start, or null when it can. */
  readonly findBlocked: string | null;
  /** Starts the search, from the selected box. */
  readonly onFind: () => void;
  /** Opens the check of the finds' characters, or null while it is open. */
  readonly onCheck: (() => void) | null;
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** A find's pixels, or an empty box until they arrive. */
function Crop({ picture, label }: { readonly picture: Picture | undefined; readonly label: string }) {
  return (
    <span className="crop">{picture === undefined ? null : <PictureCanvas picture={picture} label={label} />}</span>
  );
}

/** What a row's text field shows while it has no text. */
function placeholder(row: FoundRow): string {
  return row.reading ? 'Reading…' : 'Type what it shows';
}

/** The box that ticks every listed find, or leaves every one out, and shows a dash while only some are ticked. */
function KeepAll({
  rows,
  onKeepAll,
}: {
  readonly rows: readonly FoundRow[];
  readonly onKeepAll: (keep: boolean) => void;
}) {
  const box = useRef<HTMLInputElement>(null);
  const state = keptState(rows);
  useEffect(() => {
    if (box.current !== null) {
      box.current.indeterminate = state === 'some';
    }
  }, [state]);
  return (
    <label className="select-all">
      <input
        ref={box}
        type="checkbox"
        checked={state === 'all'}
        onChange={() => {
          onKeepAll(state !== 'all');
        }}
      />
      Select all
    </label>
  );
}

/** The inspector's part for finding more text like the selected box, and adding what the user ticks as boxes. */
export function FoundPanel({ finder, findBlocked, onFind, onCheck }: PanelProps) {
  const { view, blocked: addBlocked, error, searching, flags, crops } = finder;
  // Finds that show characters the reference does not need are left out of the list.
  const rows = listed(finder.rows);
  const reread = finder.rereadOffer;
  const unlisted = finder.rows.length - rows.length;
  const title = useRef<HTMLHeadingElement>(null);
  const find = useRef<HTMLButtonElement>(null);
  // The pressed button leaves with the panel it was on, so focus follows to what replaced it: the results'
  // heading after a search, and Find more like it after adding or dismissing them.
  const shown = view?.id ?? null;
  const last = useRef(shown);
  useEffect(() => {
    if (shown === last.current) {
      return;
    }
    const before = last.current;
    last.current = shown;
    if (shown !== null) {
      title.current?.focus();
    } else if (before !== null) {
      find.current?.focus();
    }
  }, [shown]);
  if (view === null) {
    return (
      <div className="field">
        <div className="row">
          <button
            ref={find}
            type="button"
            className="btn"
            disabled={findBlocked !== null}
            // Not disabled while searching, so focus stays on it when a search fails.
            aria-disabled={searching}
            aria-describedby={findBlocked === null ? 'hint-find-like' : 'hint-find-like find-blocked'}
            onClick={() => {
              if (!searching) {
                onFind();
              }
            }}
          >
            {searching ? 'Finding…' : 'Find more like it'}
          </button>
          <Hint topic="Find more like it" id="hint-find-like">
            {FIND_LIKE_HINT}
          </Hint>
        </div>
        {findBlocked === null ? null : (
          <p id="find-blocked" className="note">
            {findBlocked}
          </p>
        )}
        {error === null ? null : (
          <p className="warn" role="alert">
            {error}
          </p>
        )}
      </div>
    );
  }
  const kept = rows.filter((row) => row.keep).length;
  const skipped = [
    unlisted === 0
      ? null
      : `Skipped ${plural(unlisted, 'find that shows', 'finds that show')} characters the reference does not need. A find you ticked before its text was read is skipped the same way.`,
    view.otherSizes === 0 ? null : `Skipped ${plural(view.otherSizes, 'run', 'runs')} of other sizes.`,
    view.more === 0 ? null : `${plural(view.more, 'more was', 'more were')} left out; add these, then find again.`,
  ].filter((part) => part !== null);
  return (
    <section className="autofill" aria-labelledby="found-title">
      <div className="row">
        <h3 id="found-title" ref={title} tabIndex={-1}>
          {rows.length === 0 ? 'Found nothing more like it' : `Found ${rows.length} more like it`}
        </h3>
        <Hint topic="Found text">{FOUND_HINT}</Hint>
      </div>
      {rows.length === 0 ? (
        <p className="note">
          No other text on this screenshot that the boxes leave out has the same color and size and shows only the
          characters the reference needs.
        </p>
      ) : (
        <>
          <p className="note">
            Text in the same color and size on this screenshot, read as text. Tick the ones to add; each becomes a box
            the reference learns from.
          </p>
          <div className="row">
            <KeepAll rows={rows} onKeepAll={finder.keepAll} />
            {/* Each button leaves once pressed, so focus moves to the heading rather than being lost. */}
            {flags.size === 0 ? null : (
              <button
                type="button"
                className="btn small"
                onClick={() => {
                  finder.leaveOutFlagged();
                  title.current?.focus();
                }}
              >
                Deselect flagged
              </button>
            )}
            {reread === null ? null : (
              <span className="with-hint">
                <button
                  type="button"
                  className="btn small"
                  // Not disabled while it waits, so it keeps focus and a screen reader reads why.
                  aria-disabled={reread.blocked !== null}
                  aria-describedby={reread.blocked === null ? 'hint-reread' : 'hint-reread reread-blocked'}
                  onClick={() => {
                    if (reread.blocked !== null) {
                      return;
                    }
                    background(finder.reread());
                    title.current?.focus();
                  }}
                >
                  Reread
                </button>
                <Hint topic="Reread" id="hint-reread">
                  {REREAD_HINT}
                </Hint>
              </span>
            )}
          </div>
          <div className="row">
            <button
              type="button"
              className="btn small"
              disabled={onCheck === null}
              aria-describedby="hint-check-finds"
              onClick={onCheck ?? undefined}
            >
              Check characters
            </button>
            <Hint topic="Check characters" id="hint-check-finds">
              {CHECK_CHARACTERS_HINT}
            </Hint>
          </div>
          {reread === null || reread.blocked === null ? null : (
            <p id="reread-blocked" className="note">
              {reread.blocked}
            </p>
          )}
          <ul aria-label="Found text">
            {rows.map((row) => {
              const number = row.index + 1;
              const flag = flags.get(row.index);
              const flagId = `found-${row.index}-flag`;
              return (
                <li key={row.index} className={row.keep ? undefined : 'off'}>
                  <input
                    type="checkbox"
                    aria-label={`Add find ${number}`}
                    checked={row.keep}
                    onChange={(event) => {
                      finder.edit(row.index, { keep: event.target.checked, text: row.text, typed: row.typed });
                    }}
                  />
                  <span className={flag === undefined ? 'n' : 'n flagged'} aria-hidden="true">
                    {number}
                  </span>
                  <Crop picture={crops.get(row.index)} label={`Find ${number}`} />
                  <input
                    className={flag === undefined ? 'input' : 'input bad'}
                    placeholder={placeholder(row)}
                    aria-label={`Text of find ${number}`}
                    aria-invalid={flag !== undefined}
                    aria-describedby={flag === undefined ? undefined : flagId}
                    value={row.text}
                    onChange={(event) => {
                      finder.edit(row.index, {
                        keep: row.keep || event.target.value.trim() !== '',
                        text: event.target.value,
                        typed: true,
                      });
                    }}
                  />
                  {flag === undefined ? null : (
                    <span id={flagId} className="flag">
                      {flag}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        </>
      )}
      {skipped.length === 0 ? null : <p className="note">{skipped.join(' ')}</p>}
      {error === null ? null : <p className="warn">{error}</p>}
      <div className="row">
        {rows.length === 0 ? null : (
          <button
            type="button"
            className="btn primary"
            disabled={addBlocked !== null}
            aria-describedby={addBlocked === null ? undefined : 'found-blocked'}
            onClick={finder.add}
          >
            {kept === 1 ? 'Add 1 to the reference' : `Add ${kept} to the reference`}
          </button>
        )}
        <button type="button" className="btn" onClick={finder.dismiss}>
          {rows.length === 0 ? 'Close' : 'Not these'}
        </button>
      </div>
      {rows.length === 0 ? null : (
        // Kept while the list shows, so a screen reader hears each change of why Add waits.
        <p id="found-blocked" className="note pending" aria-live="polite">
          {addBlocked ?? ''}
        </p>
      )}
    </section>
  );
}

/** Numbered outlines over the finds on the screenshot they were found on, scaled from `native` to `shown`. */
export function FoundMarks({
  rows,
  flags,
  native,
  shown,
}: {
  readonly rows: readonly FoundRow[];
  readonly flags: ReadonlyMap<number, string>;
  readonly native: Size;
  readonly shown: Size;
}) {
  if (rows.length === 0 || native.width === 0 || native.height === 0) {
    return null;
  }
  const x = shown.width / native.width;
  const y = shown.height / native.height;
  return (
    <div className="found-marks" aria-hidden="true">
      {rows.map((row) => (
        <span
          key={row.index}
          className={['mark', row.keep ? null : 'off', flags.has(row.index) ? 'flagged' : null]
            .filter((name) => name !== null)
            .join(' ')}
          style={{
            left: row.region.x * x,
            top: row.region.y * y,
            width: row.region.width * x,
            height: row.region.height * y,
          }}
        >
          <span>{row.index + 1}</span>
        </span>
      ))}
    </div>
  );
}
