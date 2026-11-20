import { type ComponentProps, type ReactNode, useCallback, useRef, useState } from 'react';

import { errorText } from '../../lib/api.ts';
import type { DraftShot } from '../../lib/bindings.ts';
import { liveSizeAfter } from '../../lib/liveDraw.ts';
import type { Picture } from '../../lib/pixels.ts';
import type { Size } from '../../lib/region.ts';
import { background } from '../../lib/repeat.ts';
import { useScreenshotShortcut } from '../../lib/useScreenshotShortcut.ts';
import { useScreenshots } from '../../lib/useScreenshots.ts';
import { LiveCanvas, PictureCanvas } from '../canvas.tsx';
import { ZoomView } from '../ZoomView.tsx';
import { ScreenshotStrip } from './ScreenshotStrip.tsx';
import { StageBar } from './StageBar.tsx';

/** The widest live frame asked for, which keeps a large window's preview cheap. */
const LIVE_MAX_WIDTH = 1600;

/**
 * What a screenshot stage shows: the session's screenshots, and the live window's size and error as its frames
 * arrive. Those in `restored` came back from a draft.
 */
export function useScreenshotStage(restored: readonly DraftShot[] = []) {
  const screenshots = useScreenshots(restored);
  const [liveSize, setLiveSize] = useState<Size | null>(null);
  const [liveError, setLiveError] = useState<string | null>(null);
  // Whether the live view holds still for a press, as of the last event, for a screen that draws over it.
  const liveFrozen = useRef<() => boolean>(() => false);
  const onFrame = useCallback((width: number, height: number): void => {
    setLiveError(null);
    const frozen = liveFrozen.current();
    setLiveSize((current) => liveSizeAfter(current, { width, height }, frozen));
  }, []);
  const onLiveError = useCallback((error: unknown): void => {
    setLiveError(errorText(error));
  }, []);
  const { selected, full } = screenshots;
  /** The screenshot shown at its own pixels, or null for the live window and until the pixels arrive. */
  const shownPicture = selected === null ? null : (full.get(selected) ?? null);
  return { screenshots, liveSize, liveError, liveFrozen, onFrame, onLiveError, shownPicture };
}

interface Props {
  readonly stage: ReturnType<typeof useScreenshotStage>;
  /** What a screen reader calls the stage. */
  readonly label: string;
  /** Where the user is and what the stage shows, for its bar. */
  readonly bar: Omit<ComponentProps<typeof StageBar>, 'children'>;
  /** The screen's own buttons, before the stage's. */
  readonly actions?: ReactNode;
  /** What Take screenshot and the shortcut do: take one and show it, unless the screen does more. */
  readonly onTake?: () => Promise<unknown>;
  /** Whether there is a window to show and take screenshots of. */
  readonly watching?: boolean;
  /**
   * What shows in place of the live window while the screen has none and says so itself, and why Take screenshot
   * waits. The screenshots still show.
   */
  readonly absent?: { readonly panel: ReactNode; readonly reason: string } | null;
  /** Hears each failure of the live view, for a screen that looks for its window again once it closed. */
  readonly onLiveFailed?: (() => void) | null;
  /**
   * What shows in place of the picture, such as a recording's review: its buttons, which take the bar, and its
   * content. The picture is hidden rather than removed, so the view keeps its measured size.
   */
  readonly sheet?: { readonly actions: ReactNode; readonly content: ReactNode } | null;
  /** The screen's own warnings, under the stage's. */
  readonly notes?: ReactNode;
  /** A press over the live view: whether its frame holds still, and the screenshot it took, which shows in its place. */
  readonly held?: { readonly frozen: boolean; readonly picture: Picture | null };
  /** What the user can do on the picture now, given the screenshot shortcut as its key shows it. */
  readonly hint: (shortcut: string) => string;
  /** What the screen draws over the picture. */
  readonly children: ComponentProps<typeof ZoomView>['children'];
  /** What the strip offers and says; the stage selects what it shows. */
  readonly strip: Pick<ComponentProps<typeof ScreenshotStrip>, 'note'> & {
    /** The screenshots listed, where they are not all of them. */
    readonly shots?: ComponentProps<typeof ScreenshotStrip>['shots'];
    /** The way to remove the screenshot shown, where the screen has one. */
    readonly removal?: ComponentProps<typeof ScreenshotStrip>['removal'];
    /** Hears what the strip showed, after it shows. */
    readonly onSelect?: (index: number | null) => void;
  };
  /** What shows under the strip. */
  readonly foot?: ReactNode;
}

/**
 * A capture screen's stage: its bar, with the screenshot shortcut and Take screenshot, the live window or the
 * screenshot shown, with what the screen draws over it, and the strip that picks between them.
 */
export function ScreenshotStage({
  stage,
  label,
  bar,
  actions = null,
  onTake,
  watching = true,
  absent = null,
  onLiveFailed = null,
  sheet = null,
  notes = null,
  held = { frozen: false, picture: null },
  hint,
  children,
  strip,
  foot = null,
}: Props) {
  const { screenshots, shownPicture } = stage;
  const { selected, setSelected, position } = screenshots;
  const windowShows = watching && absent === null;
  const take = onTake ?? screenshots.take;
  const shortcut = useScreenshotShortcut(windowShows ? take : null);
  const { onLiveError } = stage;
  const liveFailed = useCallback(
    (error: unknown): void => {
      onLiveError(error);
      onLiveFailed?.();
    },
    [onLiveError, onLiveFailed],
  );
  const sheeted = sheet !== null;
  return (
    <section className="stage" aria-label={label}>
      <StageBar {...bar}>
        {sheet === null ? (
          <>
            {actions}
            <button
              type="button"
              className="btn small"
              aria-pressed={shortcut.choosing}
              onClick={shortcut.toggle}
              onBlur={shortcut.cancel}
              onKeyDown={shortcut.onKeyDown}
            >
              Change shortcut
            </button>
            <button
              type="button"
              className="btn primary"
              data-landing="screenshot"
              disabled={!windowShows}
              title={absent?.reason}
              onClick={() => {
                background(take());
              }}
            >
              Take screenshot<span className="kbd">{shortcut.label}</span>
            </button>
          </>
        ) : (
          sheet.actions
        )}
      </StageBar>
      <p className="note" role="status">
        {shortcut.choosing ? 'Press the new shortcut, or Esc to keep this one.' : null}
      </p>
      {shortcut.error === null ? null : (
        <p className="warn" role="alert">
          {shortcut.error}
        </p>
      )}
      {shortcut.saveError === null ? null : (
        <p className="warn" role="alert">
          {shortcut.saveError}
        </p>
      )}
      {screenshots.error === null ? null : (
        <p className="warn" role="alert">
          {screenshots.error}
        </p>
      )}
      {notes}
      {sheet?.content}
      {absent === null || selected !== null ? (
        <ZoomView
          picture={selected === null ? stage.liveSize : shownPicture}
          zoomable={shownPicture !== null}
          hidden={sheeted}
          hint={hint(shortcut.label)}
        >
          {(layout) => (
            <>
              {sheeted || !watching ? null : selected !== null ? (
                shownPicture === null ? null : (
                  <PictureCanvas picture={shownPicture} label={`Screenshot ${position(selected)}`} />
                )
              ) : held.picture !== null ? (
                <PictureCanvas picture={held.picture} label="The screenshot the region goes on" />
              ) : (
                <LiveCanvas
                  maxWidth={Math.min(LIVE_MAX_WIDTH, Math.max(1, Math.round(layout.space.width * devicePixelRatio)))}
                  paused={held.frozen}
                  onFrame={stage.onFrame}
                  onError={liveFailed}
                />
              )}
              {children(layout)}
            </>
          )}
        </ZoomView>
      ) : sheeted ? null : (
        absent.panel
      )}
      <ScreenshotStrip
        shots={strip.shots ?? screenshots.shots}
        selected={selected}
        live={absent === null}
        position={position}
        hidden={sheeted}
        removal={strip.removal ?? null}
        note={strip.note}
        onSelect={(index) => {
          setSelected(index);
          strip.onSelect?.(index);
        }}
      />
      {foot}
    </section>
  );
}
