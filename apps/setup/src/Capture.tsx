import { type KeyboardEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { type ShortcutEvent, register, unregister } from '@tauri-apps/plugin-global-shortcut';

import {
  type Reading,
  type RecordingView,
  discardRecording,
  errorText,
  readValue,
  recordedReading,
  recordingState,
  reviewProblems,
  saveValue,
  screenshot,
  setCheck,
  startRecording,
  stopRecording,
  takeScreenshot,
} from './api.ts';
import { readingText } from './automation.ts';
import { LiveCanvas, PictureCanvas } from './canvas.tsx';
import {
  type CheckKind,
  type Draft,
  buildSpec,
  CHECK_KINDS,
  EMPTY_DRAFT,
  KIND_NAMES,
  reviewSpec,
  withKind,
} from './check.ts';
import type { Choice } from './Picker.tsx';
import { type Color, type Picture, colorAt, hex } from './pixels.ts';
import { RecordPanel, ReviewSheet } from './Recording.tsx';
import { type ReviewEntry, clock, entries, keptLabels } from './review.ts';
import { type Point, type Rect, type Size, fit } from './region.ts';
import { RegionSurface } from './RegionSurface.tsx';
import { background, useRepeat } from './repeat.ts';
import { accelerator, shortcutLabel } from './shortcut.ts';
import { useSize } from './size.ts';
import { loadShortcut, saveShortcut } from './storage.ts';

/** A screenshot taken this session. */
interface Shot {
  readonly index: number;
  readonly time: string;
  readonly thumb: Picture | null;
}

/** Where a picked color goes. */
type ColorTarget = 'ink' | 'filled' | 'empty' | 'color';

type CheckState =
  | { readonly kind: 'idle' }
  | { readonly kind: 'pending' }
  | { readonly kind: 'set' }
  | { readonly kind: 'error'; readonly message: string };

const THUMB_WIDTH = 224;
/** Wide enough for any window, so a screenshot arrives at its own pixels. */
const FULL_WIDTH = 16_384;
/** The widest live frame asked for, which keeps a large window's preview cheap. */
const LIVE_MAX_WIDTH = 1600;
/** How long the check waits after the last change before it is tried. */
const CHECK_DELAY_MS = 250;
/** How long the live reading waits between reads. */
const READING_PAUSE_MS = 120;
/** How long the recording's state waits between reads, while it records or text is still being read. */
const RECORDING_PAUSE_MS = 250;
/** The widest a recorded reading is asked for: a region is small, and review shows it a few times larger. */
const CROP_WIDTH = 480;
/** How long review waits after the last edit before it asks which readings the check cannot learn. */
const REVIEW_DELAY_MS = 200;

function viewHint(
  recording: boolean,
  selected: number | null,
  shotCount: number,
  picking: boolean,
  liveError: string | null,
  shortcut: string,
): string {
  if (recording && selected === null) {
    return 'Recording. Use the application; each different reading is kept.';
  }
  if (selected === null) {
    if (liveError !== null) {
      return liveError;
    }
    return shotCount === 0
      ? `Take a screenshot (${shortcut}) when the window shows what to read.`
      : 'Select a screenshot to draw on it.';
  }
  if (picking) {
    return 'Click the color to pick it.';
  }
  return 'Drag to draw a region. Drag inside it to move it, or drag a corner to resize it.';
}

function RegionFields({
  region,
  onRegion,
}: {
  readonly region: Rect | null;
  readonly onRegion: (region: Rect) => void;
}) {
  const parts = ['x', 'y', 'width', 'height'] as const;
  const names = { x: 'X', y: 'Y', width: 'Width', height: 'Height' } as const;
  return (
    <div className="field">
      <span>Region, in the window&apos;s pixels</span>
      <div className="grid4">
        {parts.map((part) => (
          <input
            key={part}
            className="input"
            type="number"
            min={part === 'width' || part === 'height' ? 1 : 0}
            aria-label={names[part]}
            title={names[part]}
            disabled={region === null}
            value={region?.[part] ?? ''}
            onChange={(event) => {
              const value = Number.parseInt(event.target.value, 10);
              const low = part === 'width' || part === 'height' ? 1 : 0;
              if (region !== null && Number.isInteger(value) && value >= low) {
                onRegion({ ...region, [part]: value });
              }
            }}
          />
        ))}
      </div>
    </div>
  );
}

interface ColorProps {
  readonly label: string;
  readonly color: Color | null;
  readonly picking: boolean;
  readonly canPick: boolean;
  readonly onPick: () => void;
}

function ColorField({ label, color, picking, canPick, onPick }: ColorProps) {
  return (
    <div className="field">
      <span>{label}</span>
      <div className="colorpick">
        <span className="chip">
          {color === null ? (
            'Not picked'
          ) : (
            <>
              <span className="swatch" style={{ background: hex(color) }} />
              {hex(color)}
            </>
          )}
        </span>
        <button
          type="button"
          className="btn small"
          aria-pressed={picking}
          disabled={!canPick}
          title={canPick ? undefined : 'Select a screenshot first'}
          onClick={onPick}
        >
          {picking ? 'Cancel' : 'Pick'}
        </button>
      </div>
    </div>
  );
}

const TOLERANCE_HELP: Readonly<Record<CheckKind, string>> = {
  number: "How far a pixel's color may be from the characters' color.",
  barFill: '',
  color: "How far the region's average color may be.",
  reference: 'How far, on average, the region may differ from the screenshot.',
};

function ToleranceField({
  kind,
  value,
  onChange,
}: {
  readonly kind: CheckKind;
  readonly value: number;
  readonly onChange: (value: number) => void;
}) {
  return (
    <div className="field">
      <label htmlFor="tolerance">Tolerance</label>
      <div className="range">
        <input
          id="tolerance"
          type="range"
          min={0}
          max={255}
          value={value}
          onChange={(event) => {
            onChange(Number(event.target.value));
          }}
        />
        <output htmlFor="tolerance">{value}</output>
      </div>
      <span className="note">{TOLERANCE_HELP[kind]}</span>
    </div>
  );
}

function ReadingBox({
  built,
  state,
  reading,
}: {
  readonly built: string | null;
  readonly state: CheckState;
  readonly reading: Reading | null;
}) {
  if (built !== null) {
    return <p className="reading note">{built}</p>;
  }
  switch (state.kind) {
    case 'idle':
    case 'pending':
      return <p className="reading note">Trying the check…</p>;
    case 'error':
      return <p className="reading warn">{state.message}</p>;
    case 'set':
      return (
        <p className="reading" aria-live="polite">
          {reading === null ? (
            <small>Waiting for a frame…</small>
          ) : (
            <>
              <strong>{readingText(reading)}</strong>
              <small>on the live window</small>
            </>
          )}
        </p>
      );
  }
}

interface KindProps {
  readonly draft: Draft;
  readonly picking: ColorTarget | null;
  readonly canPick: boolean;
  readonly selected: number | null;
  readonly position: (index: number) => number;
  readonly onDraft: (update: (draft: Draft) => Draft) => void;
  readonly onPicking: (target: ColorTarget | null) => void;
}

function KindFields({ draft, picking, canPick, selected, position, onDraft, onPicking }: KindProps) {
  const color = (target: ColorTarget, label: string) => (
    <ColorField
      key={target}
      label={label}
      color={draft[target]}
      picking={picking === target}
      canPick={canPick}
      onPick={() => {
        onPicking(picking === target ? null : target);
      }}
    />
  );
  const tolerance = (
    <ToleranceField
      kind={draft.kind}
      value={draft.tolerance}
      onChange={(value) => {
        onDraft((current) => ({ ...current, tolerance: value }));
      }}
    />
  );
  switch (draft.kind) {
    case 'number': {
      const labeled = [...draft.labels.values()].filter((text) => text.trim() !== '').length;
      return (
        <>
          {color('ink', "Characters' color")}
          {tolerance}
          <div className="field">
            <label htmlFor="label-text">What the region shows on this screenshot</label>
            {selected === null ? (
              <p className="note">Select a screenshot to type what it shows, such as 87%.</p>
            ) : (
              <input
                id="label-text"
                className="input"
                placeholder="87%"
                value={draft.labels.get(selected) ?? ''}
                onChange={(event) => {
                  const text = event.target.value;
                  onDraft((current) => ({ ...current, labels: new Map(current.labels).set(selected, text) }));
                }}
              />
            )}
            <span className="note">
              {labeled === 1 ? 'Labeled on 1 screenshot.' : `Labeled on ${labeled} screenshots.`} Label screenshots that
              show every digit it should know.
            </span>
          </div>
        </>
      );
    }
    case 'barFill':
      return (
        <>
          {color('filled', 'Filled color')}
          {color('empty', 'Empty color')}
        </>
      );
    case 'color':
      return (
        <>
          {color('color', 'Color to look for')}
          {tolerance}
        </>
      );
    case 'reference':
      return (
        <>
          <div className="field">
            <span>Compare against</span>
            <div className="colorpick">
              <span className="chip">
                {draft.reference === null ? 'No screenshot chosen' : `Screenshot ${position(draft.reference)}`}
              </span>
              <button
                type="button"
                className="btn small"
                disabled={selected === null}
                onClick={() => {
                  onDraft((current) => ({ ...current, reference: selected }));
                }}
              >
                Use this one
              </button>
            </div>
          </div>
          {tolerance}
        </>
      );
  }
}

interface Props {
  readonly choice: Choice;
  readonly onCancel: () => void;
  readonly onChangeWindow: () => void;
  readonly onSaved: () => void;
}

export function Capture({ choice, onCancel, onChangeWindow, onSaved }: Props) {
  const [shots, setShots] = useState<readonly Shot[]>([]);
  const [full, setFull] = useState<ReadonlyMap<number, Picture>>(new Map());
  const [selected, setSelected] = useState<number | null>(null);
  const [liveSize, setLiveSize] = useState<Size | null>(null);
  const [liveError, setLiveError] = useState<string | null>(null);
  const [shotError, setShotError] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft>(EMPTY_DRAFT);
  const [picking, setPicking] = useState<ColorTarget | null>(null);
  const [valueName, setValueName] = useState('');
  const [checkState, setCheckState] = useState<CheckState>({ kind: 'idle' });
  const [reading, setReading] = useState<Reading | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState(loadShortcut);
  const [changingShortcut, setChangingShortcut] = useState(false);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [recording, setRecording] = useState<RecordingView | null>(null);
  const [recordError, setRecordError] = useState<string | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [edits, setEdits] = useState<ReadonlyMap<number, ReviewEntry>>(new Map());
  // Why the check cannot learn each flagged reading, for the review spec they were found on.
  const [problems, setProblems] = useState<{ readonly spec: string; readonly found: ReadonlyMap<number, string> }>({
    spec: '',
    found: new Map(),
  });
  // Bumped by each start, stop, or discard, so a poll sent before one is ignored.
  const recordingTurn = useRef(0);
  // Each recorded reading's pixels, for the recording they belong to.
  const [crops, setCrops] = useState<{ readonly id: number; readonly pictures: ReadonlyMap<number, Picture> }>({
    id: -1,
    pictures: new Map(),
  });

  const view = useRef<HTMLDivElement>(null);
  const space = useSize(view);

  const take = useCallback(async (): Promise<void> => {
    try {
      const index = await takeScreenshot();
      const time = new Date().toLocaleTimeString();
      setShots((current) => [...current, { index, time, thumb: null }]);
      setSelected(index);
      setShotError(null);
      const [thumb, picture] = await Promise.all([screenshot(index, THUMB_WIDTH), screenshot(index, FULL_WIDTH)]);
      setShots((current) => current.map((shot) => (shot.index === index ? { ...shot, thumb } : shot)));
      if (picture !== null) {
        setFull((current) => new Map(current).set(index, picture));
      }
    } catch (error: unknown) {
      setShotError(errorText(error));
    }
  }, []);

  const latestTake = useRef(take);
  useEffect(() => {
    latestTake.current = take;
  });

  // Registering and unregistering run one after another, in the order the
  // effect asked for them, so a quick unregister cannot overtake a register.
  const shortcutQueue = useRef<Promise<void>>(Promise.resolve());
  useEffect(() => {
    if (changingShortcut) {
      return undefined;
    }
    let current = true;
    const onPress = (event: ShortcutEvent): void => {
      if (event.state === 'Pressed') {
        background(latestTake.current());
      }
    };
    shortcutQueue.current = shortcutQueue.current
      .then(async () => register(shortcut, onPress))
      .then(() => {
        if (current) {
          setShortcutError(null);
        }
      })
      .catch(() => {
        if (current) {
          setShortcutError(`${shortcutLabel(shortcut)} could not be set up. Another app may use it; choose another.`);
        }
      });
    return () => {
      current = false;
      shortcutQueue.current = shortcutQueue.current
        .then(async () => unregister(shortcut))
        .catch(() => {
          // A shortcut that never registered has nothing to remove.
        });
    };
  }, [shortcut, changingShortcut]);

  const built = useMemo(() => buildSpec(draft), [draft]);
  useEffect(() => {
    if (!built.ok) {
      setCheckState({ kind: 'idle' });
      setReading(null);
      return undefined;
    }
    setCheckState({ kind: 'pending' });
    let current = true;
    const timer = setTimeout(() => {
      setCheck(built.spec)
        .then(() => {
          if (current) {
            setCheckState({ kind: 'set' });
          }
        })
        .catch((error: unknown) => {
          if (current) {
            setCheckState({ kind: 'error', message: errorText(error) });
          }
        });
    }, CHECK_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [built]);

  const readNow = useCallback(async (): Promise<void> => {
    try {
      setReading(await readValue());
    } catch {
      setReading(null);
    }
  }, []);
  useRepeat(readNow, READING_PAUSE_MS, checkState.kind === 'set');

  const readRecording = useCallback(async (): Promise<void> => {
    const turn = recordingTurn.current;
    try {
      const state = await recordingState();
      if (turn === recordingTurn.current) {
        setRecording(state);
        setRecordError(null);
      }
    } catch (error: unknown) {
      setRecordError(errorText(error));
    }
  }, []);
  const recordingLive =
    recording !== null &&
    (recording.recording || recording.readings.some((reading) => reading.text.kind === 'pending'));
  useRepeat(readRecording, RECORDING_PAUSE_MS, recordingLive);

  // Fetches the pixels of each reading that has none yet, once.
  const asked = useRef<{ id: number; readings: Set<number> }>({ id: -1, readings: new Set() });
  const readingCount = recording?.readings.length ?? 0;
  const recordingId = recording?.id ?? null;
  useEffect(() => {
    if (recordingId === null) {
      return;
    }
    if (asked.current.id !== recordingId) {
      asked.current = { id: recordingId, readings: new Set() };
    }
    for (let index = 0; index < readingCount; index += 1) {
      if (asked.current.readings.has(index)) {
        continue;
      }
      asked.current.readings.add(index);
      background(
        recordedReading(index, CROP_WIDTH).then((picture) => {
          if (picture === null) {
            return;
          }
          setCrops((current) => {
            const pictures = current.id === recordingId ? new Map(current.pictures) : new Map<number, Picture>();
            return { id: recordingId, pictures: pictures.set(index, picture) };
          });
        }),
      );
    }
  }, [recordingId, readingCount]);
  const pictures = crops.id === recordingId ? crops.pictures : new Map<number, Picture>();

  const reviewEntries = useMemo(
    () => entries(recording?.readings ?? [], edits, draft.recorded),
    [recording, edits, draft.recorded],
  );
  const keeping = useMemo(() => keptLabels(reviewEntries), [reviewEntries]);
  const toReview = useMemo(() => reviewSpec(draft, keeping), [draft, keeping]);
  const reviewKey = toReview === null ? '' : JSON.stringify(toReview);
  // The effect below runs when the spec's contents change, not on each new
  // object with the same contents, so it reads the spec through a ref.
  const reviewSpecRef = useRef(toReview);
  reviewSpecRef.current = toReview;
  // Asks which kept readings the check cannot learn, such as one whose shape
  // a labeled screenshot or an earlier reading gave to another character.
  useEffect(() => {
    const toReview = reviewSpecRef.current;
    if (!reviewing || toReview === null) {
      return undefined;
    }
    let current = true;
    const timer = setTimeout(() => {
      reviewProblems(toReview)
        .then((answers) => {
          if (!current) {
            return;
          }
          const found = new Map<number, string>();
          answers.forEach((answer, position_) => {
            const label = toReview.recorded[position_];
            if (answer !== null && label !== undefined) {
              found.set(label.reading, answer);
            }
          });
          setProblems({ spec: reviewKey, found });
          setRecordError(null);
        })
        .catch((error: unknown) => {
          if (current) {
            // Saving checks the readings again, so a failed check flags none
            // rather than leaving Keep waiting for an answer that never comes.
            setProblems({ spec: reviewKey, found: new Map() });
            setRecordError(errorText(error));
          }
        });
    }, REVIEW_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [reviewing, reviewKey]);
  // Flags found on another spec are stale: none show, and Keep waits for the answer.
  const flagged = problems.spec === reviewKey ? problems.found : new Map<number, string>();
  const checked = problems.spec === reviewKey;

  const onFrame = useCallback((width: number, height: number): void => {
    setLiveError(null);
    setLiveSize((current) => (current?.width === width && current.height === height ? current : { width, height }));
  }, []);
  const onLiveError = useCallback((error: unknown): void => {
    setLiveError(errorText(error));
  }, []);

  // The region is measured on the screenshot shown, or on the newest one
  // while the live view is shown.
  const drawnOn = selected !== null && full.has(selected) ? selected : (shots.at(-1)?.index ?? null);
  const native = drawnOn === null ? undefined : full.get(drawnOn);
  const shownPicture = selected === null ? null : (full.get(selected) ?? null);
  const layoutSize: Size | null = selected === null ? liveSize : shownPicture;
  const shown = layoutSize === null ? { width: 0, height: 0 } : fit(layoutSize, space);
  const frameStyle = {
    left: (space.width - shown.width) / 2,
    top: (space.height - shown.height) / 2,
    width: shown.width,
    height: shown.height,
  };

  const pick = (point: Point): void => {
    if (picking === null || shownPicture === null) {
      return;
    }
    const color = colorAt(shownPicture, point.x, point.y);
    if (color !== null) {
      setDraft((current) => ({ ...current, [picking]: color }));
      setPicking(null);
    }
  };

  const save = async (): Promise<void> => {
    if (!built.ok || drawnOn === null) {
      return;
    }
    setSaving(true);
    try {
      await saveValue({
        app: choice.app,
        name: choice.name,
        window: choice.matcher,
        valueName: valueName.trim(),
        check: built.spec,
        screenshot: drawnOn,
      });
      onSaved();
    } catch (error: unknown) {
      setSaveError(errorText(error));
    } finally {
      setSaving(false);
    }
  };

  const shortcutKey = (event: KeyboardEvent<HTMLButtonElement>): void => {
    if (!changingShortcut) {
      return;
    }
    event.preventDefault();
    if (event.code === 'Escape') {
      setChangingShortcut(false);
      return;
    }
    const next = accelerator(event);
    if (next !== null) {
      setShortcut(next);
      saveShortcut(next);
      setChangingShortcut(false);
    }
  };

  const recordBlocked = ((): string | null => {
    if (draft.kind !== 'number') {
      return null;
    }
    if (draft.region === null || drawnOn === null) {
      return 'Draw the region on a screenshot to record it.';
    }
    if (draft.ink === null) {
      return "Pick the characters' color to record.";
    }
    return null;
  })();

  // Closing review removes the button that had focus, so focus moves to the
  // inspector's recording part once it shows again.
  const closeReview = (): void => {
    if (!reviewing) {
      return;
    }
    setReviewing(false);
    requestAnimationFrame(() => {
      document.getElementById('record-title')?.focus();
    });
  };

  // Starts a new recording, or with `more` goes on with the stopped one. The
  // owner refuses to go on with other settings rather than start over, so
  // kept readings and review edits are never dropped without a discard.
  const record = async (more: boolean): Promise<void> => {
    if (draft.region === null || draft.ink === null || drawnOn === null) {
      return;
    }
    recordingTurn.current += 1;
    try {
      await startRecording(draft.region, draft.ink, draft.tolerance, drawnOn, more);
      setRecordError(null);
      setReviewing(false);
      setSelected(null);
      setPicking(null);
      if (!more) {
        setEdits(new Map());
        setDraft((current) => ({ ...current, recorded: new Map() }));
      }
      setRecording(await recordingState());
    } catch (error: unknown) {
      setRecordError(errorText(error));
    }
  };

  // Stops recording; `review` opens the readings to review.
  const stop = async (review: boolean): Promise<void> => {
    recordingTurn.current += 1;
    try {
      await stopRecording();
      setRecording(await recordingState());
      setReviewing(review);
    } catch (error: unknown) {
      setRecordError(errorText(error));
    }
  };

  const discard = async (): Promise<void> => {
    recordingTurn.current += 1;
    try {
      await discardRecording();
      setRecording(null);
      setRecordError(null);
      closeReview();
      setEdits(new Map());
      setDraft((current) => ({ ...current, recorded: new Map() }));
    } catch (error: unknown) {
      setRecordError(errorText(error));
    }
  };

  // Review edits stay after Keep, so reviewing again starts from them.
  const keep = (): void => {
    setDraft((current) => ({ ...current, recorded: new Map(keeping.map((label) => [label.reading, label.text])) }));
    closeReview();
  };

  // Escape leaves review without keeping; its edits stay for the next review.
  useEffect(() => {
    if (!reviewing) {
      return undefined;
    }
    const onKey = (event: globalThis.KeyboardEvent): void => {
      if (event.key === 'Escape') {
        closeReview();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  });

  // The stopped recording's settings, when the draft no longer has them, so
  // Record more can offer to take them back.
  const settings = recording?.settings;
  const settingsChanged =
    recording !== null &&
    !recording.recording &&
    settings !== undefined &&
    (draft.tolerance !== settings.tolerance ||
      JSON.stringify(draft.region) !== JSON.stringify(settings.region) ||
      JSON.stringify(draft.ink) !== JSON.stringify(settings.ink));
  const useSettings = (): void => {
    if (settings === undefined) {
      return;
    }
    setDraft((current) => ({ ...current, region: settings.region, ink: settings.ink, tolerance: settings.tolerance }));
    setRecordError(null);
  };

  const keptProblem = keeping.some((label) => flagged.has(label.reading));
  const keepBlocked = keptProblem
    ? 'Fix or leave out the flagged readings to keep them.'
    : checked
      ? null
      : 'Checking the readings…';

  const canSave = valueName.trim().length > 0 && built.ok && drawnOn !== null && checkState.kind === 'set' && !saving;
  const position = (index: number): number => shots.findIndex((shot) => shot.index === index) + 1;

  return (
    <main className="capture">
      <section className="stage" aria-label="Capture">
        <div className="bar">
          <span className="crumb">
            {reviewing ? (
              <>
                Values / New value / <b>Review recording</b>
              </>
            ) : (
              <>
                Values / <b>New value</b>
              </>
            )}
          </span>
          {recording?.recording === true ? (
            <span className="pill recording">
              <span className="dot" />
              Recording {clock(recording.elapsedMs)}
            </span>
          ) : reviewing && recording !== null ? (
            <span className="pill">
              {recording.readings.length === 1 ? '1 reading' : `${recording.readings.length} readings`} from{' '}
              {clock(recording.elapsedMs)}
            </span>
          ) : (
            <span className="pill">
              <span className="dot live" />
              Live · {choice.name}
            </span>
          )}
          {native === undefined || reviewing ? null : (
            <span className="pill">
              {native.width} × {native.height}
            </span>
          )}
          {reviewing ? (
            <div className="end">
              <button type="button" className="btn" onClick={closeReview}>
                Back
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  background(discard());
                }}
              >
                Discard recording
              </button>
              <button
                type="button"
                className="btn"
                disabled={recordBlocked !== null}
                onClick={() => {
                  background(record(true));
                }}
              >
                Record more
              </button>
              <button
                type="button"
                className="btn primary"
                disabled={keepBlocked !== null}
                aria-describedby={keepBlocked === null ? undefined : 'keep-blocked'}
                onClick={keep}
              >
                {keeping.length === 1 ? 'Keep 1 reading' : `Keep ${keeping.length} readings`}
              </button>
            </div>
          ) : (
            <div className="end">
              <button type="button" className="btn" onClick={onChangeWindow}>
                Change window
              </button>
              <button
                type="button"
                className="btn small"
                aria-pressed={changingShortcut}
                onClick={() => {
                  setChangingShortcut((current) => !current);
                }}
                onKeyDown={shortcutKey}
              >
                {changingShortcut ? 'Press the new shortcut, or Esc' : 'Change shortcut'}
              </button>
              <button
                type="button"
                className="btn primary"
                onClick={() => {
                  background(take());
                }}
              >
                Take screenshot<span className="kbd">{shortcutLabel(shortcut)}</span>
              </button>
            </div>
          )}
        </div>
        {shortcutError === null ? null : <p className="warn">{shortcutError}</p>}
        {shotError === null ? null : <p className="warn">{shotError}</p>}
        {reviewing && recording !== null ? (
          <ReviewSheet
            view={recording}
            entries={reviewEntries}
            problems={flagged}
            crops={pictures}
            blocked={keepBlocked}
            error={recordError}
            onEntry={(index, entry) => {
              setEdits((current) => new Map(current).set(index, entry));
            }}
            onKeepAll={(keepAll) => {
              // A reading still being read gets no edit, so its text and
              // keep come from the reader once it answers.
              setEdits((current) => {
                const next = new Map(current);
                reviewEntries.forEach((entry, index) => {
                  if (recording.readings[index]?.text.kind !== 'pending' || current.has(index)) {
                    next.set(index, { ...entry, keep: keepAll && entry.text.trim() !== '' });
                  }
                });
                return next;
              });
            }}
          />
        ) : null}
        {/* Hidden rather than removed during review, so the view keeps its measured size. */}
        <div className="view" ref={view} hidden={reviewing}>
          <div className="frame" style={frameStyle}>
            {reviewing ? null : selected === null ? (
              <LiveCanvas
                maxWidth={Math.min(LIVE_MAX_WIDTH, Math.max(1, Math.round(space.width * devicePixelRatio)))}
                onFrame={onFrame}
                onError={onLiveError}
              />
            ) : shownPicture === null ? null : (
              <PictureCanvas picture={shownPicture} label={`Screenshot ${position(selected)}`} />
            )}
            {native === undefined ? null : (
              <RegionSurface
                size={native}
                shown={shown}
                region={draft.region}
                recording={recording?.recording === true && selected === null}
                label={valueName.trim() === '' ? 'Region' : valueName.trim()}
                editable={shownPicture !== null}
                picking={picking !== null}
                onRegion={(region) => {
                  setDraft((current) => ({ ...current, region }));
                }}
                onPick={pick}
              />
            )}
          </div>
          <p className="hint">
            {viewHint(
              recording?.recording === true,
              selected,
              shots.length,
              picking !== null,
              liveError,
              shortcutLabel(shortcut),
            )}
          </p>
        </div>
        <div className="strip" hidden={reviewing}>
          <button
            type="button"
            className="thumb"
            aria-pressed={selected === null}
            onClick={() => {
              setSelected(null);
              setPicking(null);
            }}
          >
            <span className="shot">
              <span className="dot live" />
            </span>
            Live
          </button>
          {shots.map((shot, position_) => (
            <button
              key={shot.index}
              type="button"
              className="thumb"
              aria-pressed={selected === shot.index}
              onClick={() => {
                setSelected(shot.index);
              }}
            >
              <span className="shot">
                {shot.thumb === null ? null : <PictureCanvas picture={shot.thumb} label="" />}
              </span>
              {position_ + 1} · {shot.time}
            </button>
          ))}
          <span className="note">
            Screenshots last until you change the window. A number check learns from each screenshot you label.
          </span>
        </div>
      </section>
      <aside className="inspector" aria-label="Value">
        <div className="field">
          <label htmlFor="value-name">Value name</label>
          <input
            id="value-name"
            className="input"
            placeholder="Progress"
            value={valueName}
            onChange={(event) => {
              setValueName(event.target.value);
            }}
          />
        </div>
        <div className="field">
          <span id="check-kind">Check</span>
          <div className="seg" role="group" aria-labelledby="check-kind">
            {CHECK_KINDS.map((kind) => (
              <button
                key={kind}
                type="button"
                aria-pressed={draft.kind === kind}
                onClick={() => {
                  setDraft((current) => withKind(current, kind));
                  setPicking(null);
                  if (kind !== 'number') {
                    // Recording and review belong to a number check, so
                    // leaving it stops recording and closes review.
                    setReviewing(false);
                    if (recording?.recording === true) {
                      background(stop(false));
                    }
                  }
                }}
              >
                {KIND_NAMES[kind]}
              </button>
            ))}
          </div>
        </div>
        <RegionFields
          region={draft.region}
          onRegion={(region) => {
            setDraft((current) => ({ ...current, region }));
          }}
        />
        <KindFields
          draft={draft}
          picking={picking}
          canPick={shownPicture !== null}
          selected={selected}
          position={position}
          onDraft={setDraft}
          onPicking={setPicking}
        />
        {draft.kind === 'number' ? (
          <RecordPanel
            view={recording}
            reviewing={reviewing}
            kept={draft.recorded.size}
            keeping={keeping.length}
            blocked={recordBlocked}
            appName={choice.name}
            crops={pictures}
            error={recordError}
            onStart={() => {
              background(record(false));
            }}
            onMore={() => {
              background(record(true));
            }}
            onStop={() => {
              background(stop(true));
            }}
            onDiscard={() => {
              background(discard());
            }}
            onReview={() => {
              setReviewing(true);
            }}
            settingsChanged={settingsChanged}
            onUseSettings={useSettings}
          />
        ) : null}
        <ReadingBox built={built.ok ? null : built.next} state={checkState} reading={reading} />
        <div className="sep" />
        {saveError === null ? null : <p className="warn">{saveError}</p>}
        <p className="note">Saved values show on the Values page, under {choice.name}.</p>
        <div className="actions">
          <button type="button" className="btn" onClick={onCancel}>
            Discard
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={!canSave}
            onClick={() => {
              background(save());
            }}
          >
            Save value
          </button>
        </div>
      </aside>
    </main>
  );
}
