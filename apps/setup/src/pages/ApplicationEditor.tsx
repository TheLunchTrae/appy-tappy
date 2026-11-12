import { useCallback, useEffect, useState } from 'react';

import type { AppSummary, FallbackDef, SizeChangesDef, WindowChoice, WindowDef, WindowList } from '../lib/bindings.ts';
import { errorText, listWindows, saveApp } from '../lib/api.ts';
import { Hint } from '../components/Hint.tsx';
import {
  EXECUTABLE_NOTE,
  FALLBACK_HINT,
  FALLBACK_NAMES,
  FALLBACKS,
  FIELD_HINTS,
  FIELD_NAMES,
  FIELDS,
  MUST_MATCH_HINT,
  type MatchField,
  exactWindow,
  fallbackUseful,
  fitCount,
  requires,
  sentence,
  withMustMatch,
} from '../lib/matching.ts';
import { background } from '../lib/repeat.ts';
import { SIZE_CHANGES, SIZE_CHANGES_HINT, SIZE_CHANGE_TEXT } from '../lib/sizes.ts';
import { type Held, held, useReportHeld } from '../lib/unsaved.ts';
import { Trail } from '../components/Trail.tsx';
import { WindowsPanel, useWindowFits } from '../components/WindowsPanel.tsx';

interface Props {
  /** The application to change, or null to add one. */
  readonly existing: AppSummary | null;
  /** The way to this editor, for its breadcrumb. */
  readonly trail: readonly string[];
  readonly onCancel: () => void;
  /** Hears the saved application's ID. */
  readonly onSaved: (id: string) => void;
  /** Hears what leaving the editor would lose. */
  readonly onHeld: (held: Held) => void;
}

/** How a screen reader names the answer `option` to the size-change question, with its note. */
function choiceLabel(option: SizeChangesDef): string {
  const { name, note } = SIZE_CHANGE_TEXT[option];
  return note === null ? name : `${name}, ${note.toLowerCase()}`;
}

/** Adding an application, or changing one's name, how its window is found and how often that window changes size. */
export function ApplicationEditor({ existing, trail, onCancel, onSaved, onHeld }: Props) {
  const saved = existing?.file ?? null;
  const [list, setList] = useState<WindowList | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [listed, setListed] = useState(0);
  const [selected, setSelected] = useState<number | null>(null);
  const [name, setName] = useState(saved?.name ?? '');
  const [window, setWindow] = useState<WindowDef | null>(saved?.window ?? null);
  const [sizeChanges, setSizeChanges] = useState<SizeChangesDef>(saved?.sizeChanges ?? 'often');
  const [saveError, setSaveError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { fits, error: fitError } = useWindowFits(window, listed);
  useReportHeld(
    onHeld,
    held(
      busy,
      { name: saved?.name ?? '', window: saved?.window ?? null, sizeChanges: saved?.sizeChanges ?? 'often' },
      { name, window, sizeChanges },
    ),
  );

  const load = useCallback(async (): Promise<void> => {
    try {
      setList(await listWindows());
      setListError(null);
      setListed((count) => count + 1);
    } catch (error: unknown) {
      setListError(errorText(error));
    }
  }, []);
  useEffect(() => {
    background(load());
  }, [load]);

  const choose = (picked: WindowChoice): void => {
    setSelected(picked.handle);
    // Picking a window fills in its title, class and executable and keeps what has to match.
    setWindow((current) =>
      current === null
        ? exactWindow(picked)
        : { ...current, title: picked.title, class: picked.class, executable: picked.executable },
    );
    if (name.trim().length === 0) {
      setName(picked.title);
    }
  };

  const setPattern = (field: MatchField, text: string): void => {
    setWindow((current) => (current === null ? null : { ...current, [field]: text }));
  };

  const ready = window !== null && name.trim().length > 0 && !busy;

  const save = async (): Promise<void> => {
    if (window === null) {
      return;
    }
    setBusy(true);
    try {
      onSaved(await saveApp(existing?.id ?? null, { name: name.trim(), window, sizeChanges }));
    } catch (error: unknown) {
      setSaveError(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  const heading = saved === null ? 'New application' : `Edit ${saved.name}`;
  return (
    <main className="page">
      <Trail trail={trail} here={heading} />
      <div className="page-head">
        <h1>{heading}</h1>
        <span className="lead">Pick its window, then choose what has to match to find it again.</span>
        <div className="end">
          <button
            type="button"
            className="btn"
            onClick={() => {
              background(load());
            }}
          >
            Refresh windows
          </button>
        </div>
      </div>
      <div className="picker">
        <WindowsPanel
          windows={list?.windows ?? null}
          listError={listError}
          selected={selected}
          fits={fits}
          onChoose={choose}
        />
        <aside className="side" aria-label="Application">
          <h2>Application</h2>
          {window === null ? (
            <p className="note">Pick the application&apos;s window to fill in how AppyTappy finds it.</p>
          ) : (
            <>
              <div className="field">
                <label htmlFor="app-name">Name</label>
                <input
                  id="app-name"
                  className="input"
                  value={name}
                  onChange={(event) => {
                    setName(event.target.value);
                  }}
                />
              </div>
              <p className="note">Use * for any run of characters and ? for one. Capital letters don&apos;t matter.</p>
              <div className="group-title">
                <span id="find-by">Find its window by</span>
                <Hint topic="Must match">{MUST_MATCH_HINT}</Hint>
              </div>
              {FIELDS.map((field) => {
                const fixed = field === 'executable';
                return (
                  <div className="match-row" key={field}>
                    <span className="name">
                      <label htmlFor={`match-${field}`}>{FIELD_NAMES[field]}</label>
                      <Hint topic={FIELD_NAMES[field]} id={`hint-${field}`}>
                        {FIELD_HINTS[field].text}
                        <span className="eg">For example: {FIELD_HINTS[field].example}</span>
                      </Hint>
                    </span>
                    <input
                      id={`match-${field}`}
                      className="input"
                      aria-describedby={`hint-${field}`}
                      value={window[field]}
                      onChange={(event) => {
                        setPattern(field, event.target.value);
                      }}
                    />
                    <label className="must">
                      <input
                        type="checkbox"
                        checked={requires(window, field)}
                        disabled={fixed}
                        aria-describedby={fixed ? 'must-executable' : undefined}
                        onChange={(event) => {
                          if (field !== 'executable') {
                            setWindow(withMustMatch(window, field, event.target.checked));
                          }
                        }}
                      />
                      Must match<span className="visually-hidden"> {FIELD_NAMES[field].toLowerCase()}</span>
                    </label>
                  </div>
                );
              })}
              <p id="must-executable" className="note">
                {EXECUTABLE_NOTE}
              </p>
              {fallbackUseful(window) ? (
                <div className="field">
                  <span className="name">
                    <label htmlFor="fallback">If no window qualifies</label>
                    <Hint topic="If no window qualifies" id="hint-fallback">
                      {FALLBACK_HINT}
                    </Hint>
                  </span>
                  <select
                    id="fallback"
                    className="input"
                    aria-describedby="hint-fallback"
                    value={window.fallback}
                    onChange={(event) => {
                      const picked = FALLBACKS.find((fallback: FallbackDef) => fallback === event.target.value);
                      if (picked !== undefined) {
                        setWindow({ ...window, fallback: picked });
                      }
                    }}
                  >
                    {FALLBACKS.map((fallback) => (
                      <option key={fallback} value={fallback}>
                        {FALLBACK_NAMES[fallback]}
                      </option>
                    ))}
                  </select>
                </div>
              ) : null}
              <fieldset className="choices" aria-describedby="hint-size-changes">
                <legend className="group-title">
                  <span>How often does this window change size?</span>
                  <Hint topic="How often the window changes size" id="hint-size-changes">
                    {SIZE_CHANGES_HINT}
                  </Hint>
                </legend>
                {SIZE_CHANGES.map((option) => (
                  <label key={option} className="choice" aria-label={choiceLabel(option)}>
                    <input
                      type="radio"
                      name="size-changes"
                      aria-describedby={`size-changes-${option}`}
                      value={option}
                      checked={sizeChanges === option}
                      onChange={() => {
                        setSizeChanges(option);
                      }}
                    />
                    <span>
                      <span className="choice-name">
                        {SIZE_CHANGE_TEXT[option].name}
                        {SIZE_CHANGE_TEXT[option].note === null ? null : (
                          <small className="pill">{SIZE_CHANGE_TEXT[option].note}</small>
                        )}
                      </span>
                      <small id={`size-changes-${option}`} className="note">
                        {SIZE_CHANGE_TEXT[option].description}
                      </small>
                    </span>
                  </label>
                ))}
              </fieldset>
              <p className={fitError === null ? 'note' : 'warn'} role="status">
                {fitError === null
                  ? fitCount(fits)
                  : `Couldn't check the open windows: ${sentence(fitError)} Refresh windows to try again.`}
              </p>
            </>
          )}
          {saveError === null ? null : (
            <p className="warn" role="alert">
              {saveError}
            </p>
          )}
          <div className="actions">
            <button type="button" className="btn" disabled={busy} onClick={onCancel}>
              Cancel
            </button>
            <button
              type="button"
              className="btn primary"
              disabled={!ready}
              onClick={() => {
                background(save());
              }}
            >
              Save application
            </button>
          </div>
        </aside>
      </div>
    </main>
  );
}
