import { type KeyboardEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { type ShortcutEvent, register, unregister } from '@tauri-apps/plugin-global-shortcut';

import {
  type AppFile,
  type Reading,
  type RecordingView,
  type ReferenceDef,
  type ReferenceRef,
  discardRecording,
  errorText,
  readValue,
  recordedReading,
  recordingState,
  reviewProblems,
  listApps,
  saveReference,
  saveValue,
  screenshot,
  select,
  setCheck,
  startRecording,
  stopRecording,
  takeScreenshot,
} from '../lib/api.ts';
import { LiveCanvas, PictureCanvas } from './canvas.tsx';
import {
  type Built,
  type CheckKind,
  type Draft,
  buildCharacters,
  buildSpec,
  CHECK_KINDS,
  EMPTY_DRAFT,
  fromReference,
  KIND_NAMES,
  reviewSpec,
  toReference,
  withKind,
} from '../lib/check.ts';
import { readers } from '../lib/reference.ts';
import { type CheckState, ColorField, ReadingBox, RegionFields, ToleranceField } from './Fields.tsx';
import { Question, useQuestion } from './Question.tsx';
import { ReferenceEditor } from './ReferenceEditor.tsx';
import { ReferencePicker } from './ReferencePicker.tsx';
import type { Choice } from './Picker.tsx';
import { type Picture, colorAt } from '../lib/pixels.ts';
import { RecordPanel, ReviewSheet } from './Recording.tsx';
import { type ReviewEntry, clock, entries, keptLabels } from '../lib/review.ts';
import { type Point, type Rect, type Size, fit, sameRect } from '../lib/region.ts';
import { RegionSurface } from './RegionSurface.tsx';
import {
  type SelectStep,
  type SelectTool,
  type SelectionView,
  SELECT_TOOLS,
  TOOL_NAMES,
  ignoredPixels,
  keptCount,
  withStep,
} from '../lib/selection.ts';
import { background, useRepeat } from '../lib/repeat.ts';
import { accelerator, shortcutLabel } from '../lib/shortcut.ts';
import { useSize } from '../lib/size.ts';
import { loadShortcut, saveShortcut } from '../lib/storage.ts';
import { type Held, held, useReportHeld } from '../lib/unsaved.ts';

/** A screenshot taken this session. */
interface Shot {
  readonly index: number;
  readonly time: string;
  readonly thumb: Picture | null;
}

/** Where a picked color goes. */
type ColorTarget = 'ink' | 'filled' | 'empty' | 'color';

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
/** How long a selection waits after the last change before it is made, so a dragged slider asks once. */
const SELECT_DELAY_MS = 80;

/** How each selection tool is used, shown under the tools. */
const TOOL_HELP: Readonly<Record<SelectTool, string | null>> = {
  box: null,
  snap: 'Drag a rough box around the thing. It shrinks to fit, and the plain background around and between its parts is left out. Shift+drag adds a part; Alt+drag removes one.',
  click:
    'Click the thing. The selection spreads over similar colors and fills any holes. Shift+click adds a part; Alt+click removes one.',
};

function viewHint(
  recording: boolean,
  selected: number | null,
  shotCount: number,
  picking: boolean,
  liveError: string | null,
  shortcut: string,
  tool: SelectTool,
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
  if (tool === 'snap') {
    return 'Drag a rough box around the thing to snap to it.';
  }
  if (tool === 'click') {
    return 'Click the thing to select it.';
  }
  return 'Drag to draw a region. Drag inside it to move it, or drag a corner to resize it.';
}

function SelectFields({
  tool,
  colorMatch,
  canSelect,
  onTool,
  onColorMatch,
}: {
  readonly tool: SelectTool;
  readonly colorMatch: number;
  readonly canSelect: boolean;
  readonly onTool: (tool: SelectTool) => void;
  readonly onColorMatch: (value: number) => void;
}) {
  const help = TOOL_HELP[tool];
  const set = (value: number): void => {
    if (Number.isInteger(value) && value >= 0 && value <= 255) {
      onColorMatch(value);
    }
  };
  return (
    <>
      <div className="field">
        <span id="select-tool">Select with</span>
        <div className="seg" role="group" aria-labelledby="select-tool">
          {SELECT_TOOLS.map((option) => (
            <button
              key={option}
              type="button"
              aria-pressed={tool === option}
              onClick={() => {
                onTool(option);
              }}
            >
              {TOOL_NAMES[option].name}
              <small>{TOOL_NAMES[option].hint}</small>
            </button>
          ))}
        </div>
        {help === null ? null : <span className="note">{canSelect ? help : `Select a screenshot first. ${help}`}</span>}
      </div>
      {tool === 'box' ? null : (
        <div className="field">
          <label htmlFor="color-match">Color match</label>
          <div className="range">
            <input
              id="color-match"
              type="range"
              min={0}
              max={255}
              value={colorMatch}
              onChange={(event) => {
                set(Number(event.target.value));
              }}
            />
            <input
              className="input"
              type="number"
              min={0}
              max={255}
              aria-label="Color match"
              value={colorMatch}
              onChange={(event) => {
                set(Number.parseInt(event.target.value, 10));
              }}
            />
          </div>
          <span className="note">
            How close a color must be to count as the same. Raise it if the selection stops short; lower it if it spills
            over.
          </span>
        </div>
      )}
    </>
  );
}

const TOLERANCE_HELP: Readonly<Record<CheckKind, string>> = {
  number: '',
  barFill: '',
  color: "How far the region's average color may be.",
  image: 'How far, on average, the region may differ from the screenshot.',
};

/** What the image check's selection leaves of the region. */
interface Masking {
  /** Whether the last steps selected anything. */
  readonly found: boolean;
  /** How many of the region's pixels are compared, or null without a selection. */
  readonly kept: number | null;
}

interface KindProps {
  readonly draft: Draft;
  readonly masking: Masking;
  readonly picking: ColorTarget | null;
  readonly canPick: boolean;
  readonly selected: number | null;
  readonly position: (index: number) => number;
  readonly onDraft: (update: (draft: Draft) => Draft) => void;
  readonly onPicking: (target: ColorTarget | null) => void;
}

/** The fields of a check that is not a number, which reads with a reference instead. */
function KindFields({ draft, masking, picking, canPick, selected, position, onDraft, onPicking }: KindProps) {
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
      help={TOLERANCE_HELP[draft.kind]}
      value={draft.tolerance}
      onChange={(value) => {
        onDraft((current) => ({ ...current, tolerance: value }));
      }}
    />
  );
  switch (draft.kind) {
    case 'number':
      return null;
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
    case 'image':
      return (
        <>
          <div className="field">
            <span>Compare against</span>
            <div className="colorpick">
              <span className="chip">
                {draft.image === null ? 'No screenshot chosen' : `Screenshot ${position(draft.image)}`}
              </span>
              <button
                type="button"
                className="btn small"
                disabled={selected === null}
                onClick={() => {
                  // Steps taken on another screenshot would select something
                  // else on this one, so the selection starts over.
                  onDraft((current) =>
                    current.image === selected ? current : { ...current, image: selected, steps: [] },
                  );
                }}
              >
                Use this one
              </button>
            </div>
          </div>
          {draft.steps.length === 0 ? null : (
            <div className="field">
              <label className="toggle-row">
                <input
                  type="checkbox"
                  role="switch"
                  className="switch"
                  aria-checked={draft.ignoreBackground}
                  checked={draft.ignoreBackground}
                  onChange={(event) => {
                    const ignoreBackground = event.target.checked;
                    onDraft((current) => ({ ...current, ignoreBackground }));
                  }}
                />
                Ignore the background
              </label>
              {masking.found ? (
                <span className="note">
                  {draft.ignoreBackground && masking.kept !== null && draft.region !== null
                    ? `${masking.kept.toLocaleString()} of ${(draft.region.width * draft.region.height).toLocaleString()} pixels compared. `
                    : null}
                  Shaded pixels are skipped, so the picture still matches when what is behind it changes.
                </span>
              ) : (
                <span className="note warn">Nothing stood out there. Try again, or raise Color match.</span>
              )}
            </div>
          )}
          {tolerance}
        </>
      );
  }
}

/** What the capture screen makes: a value, or a reference, new or saved, on the References page. */
export type CaptureMode =
  | { readonly kind: 'value' }
  | { readonly kind: 'reference'; readonly existing: ReferenceDef | null };

/** The reference being made or changed, and its name as typed. */
interface Editing {
  readonly existing: ReferenceDef | null;
  readonly name: string;
  /** The value's region when the editor opened from a value, to put back when it closes. */
  readonly valueRegion: Rect | null;
}

/** The value's two steps: where it shows, then how to read it. */
type Step = 'where' | 'how';

interface Props {
  readonly choice: Choice;
  readonly mode: CaptureMode;
  readonly onCancel: () => void;
  readonly onChangeWindow: () => void;
  readonly onSaved: () => void;
  /** Hears what leaving the capture would lose. */
  readonly onHeld: (held: Held) => void;
}

export function Capture({ choice, mode, onCancel, onChangeWindow, onSaved, onHeld }: Props) {
  // What the capture shows before the user does anything, which leaving it loses nothing of.
  const [start] = useState(() => {
    const editing: Editing | null =
      mode.kind === 'reference'
        ? { existing: mode.existing, name: mode.existing?.name ?? '', valueRegion: null }
        : null;
    return {
      screenshots: 0,
      recording: false,
      valueName: '',
      editing,
      draft: editing === null ? EMPTY_DRAFT : toReference(EMPTY_DRAFT, editing.existing),
    };
  });
  const [shots, setShots] = useState<readonly Shot[]>([]);
  const [full, setFull] = useState<ReadonlyMap<number, Picture>>(new Map());
  const [selected, setSelected] = useState<number | null>(null);
  const [liveSize, setLiveSize] = useState<Size | null>(null);
  const [liveError, setLiveError] = useState<string | null>(null);
  const [shotError, setShotError] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft>(start.draft);
  // The saved application the value or reference belongs to.
  const appId = choice.app;
  const [file, setFile] = useState<AppFile | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [step, setStep] = useState<Step>('where');
  const [editing, setEditing] = useState<Editing | null>(start.editing);
  const [picking, setPicking] = useState<ColorTarget | null>(null);
  const [tool, setTool] = useState<SelectTool>('box');
  // The selection the steps last made, for the steps it was made from.
  const [selection, setSelection] = useState<{
    readonly steps: readonly SelectStep[];
    readonly view: SelectionView | null;
  } | null>(null);
  const [selectError, setSelectError] = useState<string | null>(null);
  const [valueName, setValueName] = useState(start.valueName);
  const [checkState, setCheckState] = useState<CheckState>({ kind: 'idle' });
  const [reading, setReading] = useState<Reading | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState(loadShortcut);
  const [changingShortcut, setChangingShortcut] = useState(false);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [recording, setRecording] = useState<RecordingView | null>(null);
  // Each error clears only when what set it next succeeds: a start, stop or
  // discard; reading the recording's state; or asking review's problems.
  const [commandError, setRecordError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  const [problemsError, setProblemsError] = useState<string | null>(null);
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

  const holds = held(saving, start, {
    screenshots: shots.length,
    recording: recording !== null,
    valueName,
    editing,
    draft,
  });
  useReportHeld(onHeld, holds);
  const [leavingWindow, setLeavingWindow] = useState(false);
  const leaveQuestion = useQuestion<HTMLButtonElement>(leavingWindow);
  // A recording samples the window until it stops, so leaving the capture stops it.
  useEffect(
    () => () => {
      background(stopRecording());
    },
    [],
  );

  // The application's saved file, for its references and the values that read with them.
  const loadFile = useCallback(async (app: string | null): Promise<void> => {
    if (app === null) {
      setFile(null);
      return;
    }
    try {
      const found = (await listApps()).find((summary) => summary.id === app);
      setFile(found?.file ?? null);
      setFileError(found === undefined || found.file !== null ? null : found.error);
    } catch (error: unknown) {
      setFileError(errorText(error));
    }
  }, []);
  useEffect(() => {
    background(loadFile(choice.app));
  }, [loadFile, choice.app]);

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

  // The saved reference the editor adds to, or null for a new one or while no editor is open.
  const existingId = editing?.existing?.id ?? null;
  const editingRef = useMemo(
    (): ReferenceRef | null => (existingId === null ? null : { app: appId, id: existingId }),
    [existingId, appId],
  );
  const isEditing = editing !== null;
  const built: Built = useMemo(
    () => (isEditing ? buildCharacters(draft, editingRef) : buildSpec(draft, appId)),
    [isEditing, draft, editingRef, appId],
  );
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

  // Makes the selection the steps describe on the screenshot the picture is
  // compared against, and fits the region to it.
  const { image, colorMatch, steps } = draft;
  // The box the region was last fitted to, and the steps it came from, so a
  // new color match refits a region the user has not changed by hand.
  const fitted = useRef<{ readonly steps: readonly SelectStep[]; readonly region: Rect } | null>(null);
  const shownRegion = useRef(draft.region);
  shownRegion.current = draft.region;
  useEffect(() => {
    if (image === null || steps.length === 0) {
      setSelection(null);
      return undefined;
    }
    let current = true;
    const timer = setTimeout(() => {
      select(image, { tolerance: colorMatch, steps })
        .then((view) => {
          if (!current) {
            return;
          }
          setSelectError(null);
          setSelection({ steps, view });
          const last = fitted.current;
          const untouched = last === null || last.steps !== steps || sameRect(shownRegion.current, last.region);
          if (view !== null && untouched) {
            fitted.current = { steps, region: view.region };
            setDraft((latest) => ({ ...latest, region: view.region }));
          }
        })
        .catch((error: unknown) => {
          if (current) {
            setSelectError(errorText(error));
          }
        });
    }, SELECT_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [image, colorMatch, steps]);
  const selecting = draft.kind === 'image' ? tool : 'box';
  const madeView = selection?.steps === steps ? selection.view : null;
  const ignored = useMemo(
    () =>
      draft.kind === 'image' && draft.ignoreBackground && madeView !== null && draft.region !== null
        ? ignoredPixels(madeView, draft.region)
        : null,
    [draft.kind, draft.ignoreBackground, madeView, draft.region],
  );
  const masking = {
    found: selection === null || selection.steps !== steps || selection.view !== null,
    kept: ignored === null ? null : keptCount(ignored),
  };
  const addStep = (step: SelectStep): void => {
    if (selected === null) {
      return;
    }
    setDraft((current) => {
      const steps = withStep(current.steps, current.image, selected, step);
      return steps === null ? current : { ...current, steps, image: selected };
    });
  };

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
        setPollError(null);
      }
    } catch (error: unknown) {
      setPollError(errorText(error));
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
  const toReview = useMemo(() => reviewSpec(draft, keeping, editingRef), [draft, keeping, editingRef]);
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
          setProblemsError(null);
        })
        .catch((error: unknown) => {
          if (current) {
            // Saving checks the readings again, so a failed check flags none
            // rather than leaving Keep waiting for an answer that never comes.
            setProblems({ spec: reviewKey, found: new Map() });
            setProblemsError(errorText(error));
          }
        });
    }, REVIEW_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [reviewing, reviewKey]);
  // Flags found on another spec are stale: none show, and Keep waits for the answer.
  const recordError = commandError ?? problemsError ?? pollError;
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
    if (editing === null) {
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
  const focusRecordPanel = (): void => {
    requestAnimationFrame(() => {
      document.getElementById('record-title')?.focus();
    });
  };
  const closeReview = (): void => {
    if (!reviewing) {
      return;
    }
    setReviewing(false);
    focusRecordPanel();
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
      setRecordError(null);
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
      setPollError(null);
      setProblemsError(null);
      setReviewing(false);
      // Discarding removes the button that had focus, wherever it was.
      focusRecordPanel();
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
    // The pressed button gives way to Record more, so focus moves to it.
    requestAnimationFrame(() => {
      document.querySelector<HTMLButtonElement>('[data-record-more]')?.focus();
    });
  };

  // Opens the reference editor on `existing`, or on a new reference read
  // from the value's region.
  const openEditor = (existing: ReferenceDef | null): void => {
    setEditing({ existing, name: existing?.name ?? '', valueRegion: draft.region });
    setDraft((current) => toReference(current, existing));
    setPicking(null);
    setSaveError(null);
  };

  // Leaves the reference editor for the value, forgetting its labels and
  // recording and putting the value's region back.
  const closeEditor = (): void => {
    if (recording !== null) {
      background(discard());
    }
    const valueRegion = editing?.valueRegion ?? null;
    setEditing(null);
    setPicking(null);
    setDraft((current) => fromReference(current, valueRegion));
  };

  const saveRef = async (): Promise<void> => {
    if (editing === null || !built.ok || built.spec.kind !== 'characters') {
      return;
    }
    setSaving(true);
    try {
      const saved = await saveReference({
        app: appId,
        referenceName: editing.name.trim(),
        check: built.spec,
      });
      setSaveError(null);
      if (mode.kind === 'reference') {
        onSaved();
        return;
      }
      await loadFile(saved.app);
      closeEditor();
      setDraft((current) => ({ ...current, kind: 'number', reference: saved.reference }));
    } catch (error: unknown) {
      setSaveError(errorText(error));
    } finally {
      setSaving(false);
    }
  };

  const keptProblem = keeping.some((label) => flagged.has(label.reading));
  const keepBlocked = keptProblem
    ? 'Fix or leave out the flagged readings to keep them.'
    : checked
      ? null
      : 'Checking the readings…';

  const canSave = valueName.trim().length > 0 && built.ok && drawnOn !== null && checkState.kind === 'set' && !saving;
  const canSaveRef =
    editing !== null && editing.name.trim().length > 0 && built.ok && checkState.kind === 'set' && !saving;
  const whereDone = valueName.trim().length > 0 && draft.region !== null;
  const editedName = editing === null || editing.name.trim() === '' ? 'New reference' : editing.name.trim();
  const position = (index: number): number => shots.findIndex((shot) => shot.index === index) + 1;

  return (
    <main className="capture">
      <section className="stage" aria-label="Capture">
        <div className="bar">
          <span className="crumb">
            {mode.kind === 'reference' ? 'References' : 'Values'}
            {mode.kind === 'value' && editing !== null ? ' / New value' : ''} /{' '}
            {reviewing ? (
              <>
                {editedName} / <b>Review recording</b>
              </>
            ) : (
              <b>{editing === null ? 'New value' : editedName}</b>
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
              {settingsChanged ? (
                <button type="button" className="btn" onClick={useSettings}>
                  Use its settings
                </button>
              ) : (
                <button
                  type="button"
                  className="btn"
                  data-record-more
                  disabled={recordBlocked !== null}
                  onClick={() => {
                    background(record(true));
                  }}
                >
                  Record more
                </button>
              )}
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
              {leavingWindow ? (
                <Question
                  refs={leaveQuestion.refs}
                  label="Change window?"
                  keepLabel="Keep this window"
                  actLabel="Change window"
                  onKeep={() => {
                    setLeavingWindow(false);
                  }}
                  onAct={onChangeWindow}
                >
                  Change window? The screenshots, everything set up on them, and any recording are lost.
                </Question>
              ) : (
                <button
                  ref={leaveQuestion.control}
                  type="button"
                  className="btn"
                  disabled={saving}
                  onClick={() => {
                    if (holds === 'unsaved') {
                      setLeavingWindow(true);
                    } else {
                      onChangeWindow();
                    }
                  }}
                >
                  Change window
                </button>
              )}
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
                label={editing !== null ? editedName : valueName.trim() === '' ? 'Region' : valueName.trim()}
                editable={shownPicture !== null}
                picking={picking !== null}
                tool={selecting}
                ignored={ignored}
                onRegion={(region) => {
                  setDraft((current) => ({ ...current, region }));
                }}
                onPick={pick}
                onSnap={(rough, combine) => {
                  addStep({ kind: 'snap', region: rough, combine });
                }}
                onSelectAt={(point, combine) => {
                  addStep({ kind: 'click', x: point.x, y: point.y, combine });
                }}
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
              selecting,
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
            Screenshots last until you change the window.
            {editing === null ? '' : ' A reference learns from each screenshot you label.'}
          </span>
        </div>
      </section>
      {editing === null ? (
        <aside className="inspector" aria-label="Value">
          <div className="stepper" role="group" aria-label="Steps">
            <button
              type="button"
              className="step-tab"
              aria-current={step === 'where' ? 'step' : undefined}
              onClick={() => {
                setStep('where');
              }}
            >
              <span className={whereDone && step !== 'where' ? 'num done' : 'num'}>
                {whereDone && step !== 'where' ? '✓' : '1'}
              </span>
              Where to read
            </button>
            <button
              type="button"
              className="step-tab"
              aria-current={step === 'how' ? 'step' : undefined}
              onClick={() => {
                setStep('how');
              }}
            >
              <span className="num">2</span>
              How to read it
            </button>
          </div>
          {step === 'where' ? (
            <>
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
                      }}
                    >
                      {KIND_NAMES[kind]}
                    </button>
                  ))}
                </div>
              </div>
              {draft.kind === 'image' ? (
                <SelectFields
                  tool={tool}
                  colorMatch={draft.colorMatch}
                  canSelect={shownPicture !== null}
                  onTool={(next) => {
                    setTool(next);
                    setPicking(null);
                  }}
                  onColorMatch={(value) => {
                    setDraft((current) => ({ ...current, colorMatch: value }));
                  }}
                />
              ) : null}
              {selectError === null ? null : <p className="warn">{selectError}</p>}
              <RegionFields
                region={draft.region}
                onRegion={(region) => {
                  setDraft((current) => ({ ...current, region }));
                }}
              />
              <p className="note">
                Draw the region where the value shows, on any screenshot of {choice.name}. Only where it is matters
                here; how to read it comes next.
              </p>
              <div className="sep" />
              <div className="actions">
                <button type="button" className="btn" disabled={saving} onClick={onCancel}>
                  Discard
                </button>
                <button
                  type="button"
                  className="btn primary"
                  onClick={() => {
                    setStep('how');
                  }}
                >
                  Next: how to read it
                </button>
              </div>
            </>
          ) : (
            <>
              {draft.kind === 'number' ? (
                <ReferencePicker
                  appName={choice.name}
                  file={file}
                  selected={draft.reference}
                  hasRegion={draft.region !== null}
                  whole={draft.whole}
                  onSelect={(reference) => {
                    setDraft((current) => ({ ...current, reference }));
                  }}
                  onEdit={(id) => {
                    openEditor(file?.references.find((reference) => reference.id === id) ?? null);
                  }}
                  onNew={() => {
                    openEditor(null);
                  }}
                  onWhole={(whole) => {
                    setDraft((current) => ({ ...current, whole }));
                  }}
                />
              ) : (
                <KindFields
                  draft={draft}
                  masking={masking}
                  picking={picking}
                  canPick={shownPicture !== null}
                  selected={selected}
                  position={position}
                  onDraft={setDraft}
                  onPicking={setPicking}
                />
              )}
              {fileError === null ? null : <p className="warn">{fileError}</p>}
              <ReadingBox built={built.ok ? null : built.next} state={checkState} reading={reading} />
              <div className="sep" />
              {saveError === null ? null : <p className="warn">{saveError}</p>}
              <p className="note">Saved values show on the Values page, under {choice.name}.</p>
              <div className="actions">
                <button
                  type="button"
                  className="btn"
                  onClick={() => {
                    setStep('where');
                  }}
                >
                  Back
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
            </>
          )}
        </aside>
      ) : (
        <aside className="inspector" aria-label="Reference">
          <ReferenceEditor
            name={editing.name}
            appName={choice.name}
            ink={draft.ink}
            tolerance={draft.tolerance}
            picking={picking === 'ink'}
            canPick={shownPicture !== null}
            selected={selected}
            label={selected === null ? '' : (draft.labels.get(selected) ?? '')}
            saved={editing.existing?.samples.map((sample) => sample.text) ?? []}
            labeled={[...draft.labels.values()].filter((text) => text.trim() !== '')}
            recorded={[...draft.recorded.values()]}
            readers={editing.existing === null || file === null ? [] : readers(file, editing.existing.id)}
            onName={(name) => {
              setEditing((current) => (current === null ? null : { ...current, name }));
            }}
            onPick={() => {
              setPicking(picking === 'ink' ? null : 'ink');
            }}
            onTolerance={(tolerance) => {
              setDraft((current) => ({ ...current, tolerance }));
            }}
            onLabel={(text) => {
              if (selected !== null) {
                setDraft((current) => ({ ...current, labels: new Map(current.labels).set(selected, text) }));
              }
            }}
            record={
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
            }
          />
          <ReadingBox built={built.ok ? null : built.next} state={checkState} reading={reading} />
          <div className="sep" />
          {saveError === null ? null : <p className="warn">{saveError}</p>}
          <div className="actions">
            <button
              type="button"
              className="btn"
              disabled={saving}
              onClick={() => {
                if (mode.kind === 'reference') {
                  onCancel();
                } else {
                  closeEditor();
                }
              }}
            >
              Discard
            </button>
            <button
              type="button"
              className="btn primary"
              disabled={!canSaveRef}
              onClick={() => {
                background(saveRef());
              }}
            >
              Save reference
            </button>
          </div>
        </aside>
      )}
    </main>
  );
}
