import { type ReactNode, useState } from 'react';

import type { RecordingView } from '../../lib/bindings.ts';
import { clock } from '../../lib/review.ts';
import type { Size } from '../../lib/region.ts';
import type { useReview } from '../../lib/useReview.ts';
import type { useScreenshotShortcut } from '../../lib/useScreenshotShortcut.ts';
import { Question, useQuestion } from '../Question.tsx';

/** What the stage shows now, for the bar's pill. */
type StageStatus =
  | { readonly kind: 'recording'; readonly elapsedMs: number }
  | { readonly kind: 'reviewing'; readonly recording: RecordingView }
  /** The check of the characters is open, at step `step` of `of`, or past its steps. */
  | { readonly kind: 'checking'; readonly step: { readonly step: number; readonly of: number } | null }
  | { readonly kind: 'live'; readonly appName: string };

interface BarProps {
  /** The flows the current one is under, outermost first. */
  readonly path: readonly string[];
  /** What the stage works on now. */
  readonly current: string;
  readonly status: StageStatus;
  /** The size of the picture the region shows over, or null to show none. */
  readonly size: Size | null;
  /** The bar's buttons. */
  readonly children: ReactNode;
}

/** The stage's bar: where the user is, what the stage shows, and its buttons. */
export function StageBar({ path, current, status, size, children }: BarProps) {
  return (
    <div className="bar">
      <span className="crumb">
        {path.join(' / ')} / <b>{current}</b>
      </span>
      {status.kind === 'recording' ? (
        <span className="pill recording">
          <span className="dot" />
          Recording {clock(status.elapsedMs)}
        </span>
      ) : status.kind === 'checking' ? (
        status.step === null ? null : (
          <span className="pill">
            Character {status.step.step} of {status.step.of}
          </span>
        )
      ) : status.kind === 'reviewing' ? (
        <span className="pill">
          {status.recording.readings.length === 1 ? '1 reading' : `${status.recording.readings.length} readings`} from{' '}
          {clock(status.recording.elapsedMs)}
        </span>
      ) : (
        <span className="pill">
          <span className="dot live" />
          Live · {status.appName}
        </span>
      )}
      {size === null ? null : (
        <span className="pill">
          {size.width} × {size.height}
        </span>
      )}
      <div className="end">{children}</div>
    </div>
  );
}

interface ReviewProps {
  readonly review: ReturnType<typeof useReview>;
  /** Whether the draft's settings differ from the recording's, so Record more gives way to taking them back. */
  readonly settingsChanged: boolean;
  /** Why recording cannot go on, or null. */
  readonly recordBlocked: string | null;
  readonly onDiscard: () => void;
  readonly onUseSettings: () => void;
  readonly onRecordMore: () => void;
}

/** The bar's buttons while a recording's readings are reviewed. */
export function ReviewActions({
  review,
  settingsChanged,
  recordBlocked,
  onDiscard,
  onUseSettings,
  onRecordMore,
}: ReviewProps) {
  return (
    <>
      <button type="button" className="btn" onClick={review.close}>
        Back
      </button>
      <button type="button" className="btn" onClick={onDiscard}>
        Discard recording
      </button>
      {settingsChanged ? (
        <button type="button" className="btn" onClick={onUseSettings}>
          Use its settings
        </button>
      ) : (
        <button type="button" className="btn" data-record-more disabled={recordBlocked !== null} onClick={onRecordMore}>
          Record more
        </button>
      )}
      <button
        type="button"
        className="btn primary"
        disabled={review.keepBlocked !== null}
        aria-describedby={
          review.keepBlocked === null && review.untaught.readings.length === 0 ? undefined : 'keep-blocked'
        }
        onClick={review.keep}
      >
        {review.keeping.length === 1 ? 'Keep 1 reading' : `Keep ${review.keeping.length} readings`}
      </button>
    </>
  );
}

interface WindowProps {
  readonly busy: boolean;
  /** What changing the window would lose, which it asks about first: unsaved work, a draft's screenshots, or nothing. */
  readonly loses: 'unsaved' | 'draft' | 'nothing';
  readonly shortcut: ReturnType<typeof useScreenshotShortcut>;
  readonly onChangeWindow: () => void;
  readonly onTake: () => void;
}

/** The bar's buttons while the stage shows the window: change it, change the shortcut, and take a screenshot. */
export function WindowActions({ busy, loses, shortcut, onChangeWindow, onTake }: WindowProps) {
  const [leaving, setLeaving] = useState(false);
  const question = useQuestion<HTMLButtonElement>(leaving);
  return (
    <>
      {leaving ? (
        <Question
          refs={question.refs}
          label="Change window?"
          keepLabel="Keep this window"
          actLabel="Change window"
          busy={busy}
          onKeep={() => {
            setLeaving(false);
          }}
          onAct={onChangeWindow}
        >
          {loses === 'unsaved'
            ? 'Change window? The screenshots, everything set up on them, and any recording are lost.'
            : 'Change window? This starts over on the new window. The draft stays as it is unless you save it again.'}
        </Question>
      ) : (
        <button
          ref={question.control}
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => {
            if (loses === 'nothing') {
              onChangeWindow();
            } else {
              setLeaving(true);
            }
          }}
        >
          Change window
        </button>
      )}
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
      <button type="button" className="btn primary" onClick={onTake}>
        Take screenshot<span className="kbd">{shortcut.label}</span>
      </button>
    </>
  );
}
