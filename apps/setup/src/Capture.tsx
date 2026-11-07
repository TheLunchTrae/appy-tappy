import { type KeyboardEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { register, unregister } from '@tauri-apps/plugin-global-shortcut';

import { type Reading, errorText, readValue, saveValue, screenshot, setCheck, takeScreenshot } from './api.ts';
import { LiveCanvas, PictureCanvas } from './canvas.tsx';
import { type CheckKind, type Draft, buildSpec, CHECK_KINDS, EMPTY_DRAFT, KIND_NAMES, withKind } from './check.ts';
import type { Choice } from './Picker.tsx';
import { type Color, type Picture, colorAt, hex } from './pixels.ts';
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

function viewHint(
  selected: number | null,
  shotCount: number,
  picking: boolean,
  liveError: string | null,
  shortcut: string,
): string {
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

function formatReading(reading: Reading): string {
  switch (reading.kind) {
    case 'number':
      return Number.isInteger(reading.value) ? String(reading.value) : reading.value.toFixed(1);
    case 'flag':
      return reading.value ? 'Yes' : 'No';
  }
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
              <strong>{formatReading(reading)}</strong>
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
  const [recording, setRecording] = useState(false);
  const [shortcutError, setShortcutError] = useState<string | null>(null);

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

  useEffect(() => {
    if (recording) {
      return undefined;
    }
    let current = true;
    register(shortcut, (event) => {
      if (event.state === 'Pressed') {
        background(latestTake.current());
      }
    })
      .then(() => {
        if (current) {
          setShortcutError(null);
        } else {
          background(unregister(shortcut));
        }
      })
      .catch(() => {
        if (current) {
          setShortcutError(`${shortcutLabel(shortcut)} could not be set up. Another app may use it; choose another.`);
        }
      });
    return () => {
      current = false;
      unregister(shortcut).catch(() => {
        // A shortcut that never registered has nothing to remove.
      });
    };
  }, [shortcut, recording]);

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

  const onFrame = useCallback((width: number, height: number): void => {
    setLiveError(null);
    setLiveSize((current) => (current?.width === width && current.height === height ? current : { width, height }));
  }, []);
  const onLiveError = useCallback((error: unknown): void => {
    setLiveError(errorText(error));
  }, []);

  const lastShot = shots.at(-1);
  const native =
    (selected === null ? undefined : full.get(selected)) ??
    (lastShot === undefined ? undefined : full.get(lastShot.index));
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
    if (!built.ok) {
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
      });
      onSaved();
    } catch (error: unknown) {
      setSaveError(errorText(error));
    } finally {
      setSaving(false);
    }
  };

  const recordKey = (event: KeyboardEvent<HTMLButtonElement>): void => {
    if (!recording) {
      return;
    }
    event.preventDefault();
    if (event.code === 'Escape') {
      setRecording(false);
      return;
    }
    const next = accelerator(event);
    if (next !== null) {
      setShortcut(next);
      saveShortcut(next);
      setRecording(false);
    }
  };

  const canSave = valueName.trim().length > 0 && built.ok && checkState.kind === 'set' && !saving;
  const position = (index: number): number => shots.findIndex((shot) => shot.index === index) + 1;

  return (
    <main className="capture">
      <section className="stage" aria-label="Capture">
        <div className="bar">
          <span className="crumb">
            Values / <b>New value</b>
          </span>
          <span className="pill">
            <span className="dot live" />
            Live · {choice.name}
          </span>
          {native === undefined ? null : (
            <span className="pill">
              {native.width} × {native.height}
            </span>
          )}
          <div className="end">
            <button type="button" className="btn" onClick={onChangeWindow}>
              Change window
            </button>
            <button
              type="button"
              className="btn small"
              aria-pressed={recording}
              onClick={() => {
                setRecording((current) => !current);
              }}
              onKeyDown={recordKey}
            >
              {recording ? 'Press the new shortcut, or Esc' : 'Change shortcut'}
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
        </div>
        {shortcutError === null ? null : <p className="warn">{shortcutError}</p>}
        {shotError === null ? null : <p className="warn">{shotError}</p>}
        <div className="view" ref={view}>
          <div className="frame" style={frameStyle}>
            {selected === null ? (
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
            {viewHint(selected, shots.length, picking !== null, liveError, shortcutLabel(shortcut))}
          </p>
        </div>
        <div className="strip">
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
            placeholder="Level percent"
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
