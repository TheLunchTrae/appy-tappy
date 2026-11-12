import { type ReactNode, useState } from 'react';

import { Question, useQuestion } from './Question.tsx';
import { type CaptureSetup, keptSetup } from '../lib/captureDraft.ts';
import { type Resumed, unfinishedText } from '../lib/drafts.ts';
import { background } from '../lib/repeat.ts';

/** The mark on a draft's row: Draft for a new item, Draft changes for a saved one that has a draft. */
export function DraftPill({ changes = false }: { readonly changes?: boolean }) {
  return <span className={changes ? 'pill changes' : 'pill draft'}>{changes ? 'Draft changes' : 'Draft'}</span>;
}

/** What a draft row says under its name: what is still to do, or why it cannot be finished. */
export function DraftNote({
  missing,
  problem,
}: {
  readonly missing: readonly string[];
  readonly problem: string | null;
}) {
  return <small className="warn draft-missing">{problem ?? unfinishedText(missing)}</small>;
}

interface ActionsProps {
  /** The draft's name, which the buttons name for screen readers. */
  readonly name: string;
  /** Why it cannot be finished, which leaves only Delete, or null. */
  readonly problem: string | null;
  readonly onFinish: () => void;
  /** Deletes the draft; the page shows why when it fails. */
  readonly onDelete: () => Promise<void>;
}

/** A draft row's buttons: Finish, which opens its flow, and Delete, which asks first in the buttons' place. */
export function DraftActions({ name, problem, onFinish, onDelete }: ActionsProps) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const { control, refs } = useQuestion<HTMLButtonElement>(confirming);
  if (confirming) {
    return (
      <Question
        refs={refs}
        label={`Delete the draft ${name}?`}
        keepLabel="Keep"
        actLabel="Delete draft"
        busy={busy}
        onKeep={() => {
          setConfirming(false);
        }}
        onAct={() => {
          setBusy(true);
          background(
            onDelete().finally(() => {
              setBusy(false);
              setConfirming(false);
            }),
          );
        }}
      >
        Delete the draft {name}? What it has set up is lost.
      </Question>
    );
  }
  return (
    <span className="confirm">
      {problem === null ? (
        <button type="button" className="btn small primary" onClick={onFinish}>
          Finish<span className="visually-hidden"> {name}</span>
        </button>
      ) : null}
      <button
        ref={control}
        type="button"
        className="btn small"
        onClick={() => {
          setConfirming(true);
        }}
      >
        Delete<span className="visually-hidden"> the draft {name}</span>
      </button>
    </span>
  );
}

interface CaptureRowProps {
  readonly resumed: Resumed;
  readonly problem: string | null;
  /** The cells between the name and the buttons, which differ from page to page. */
  readonly cells: (setup: CaptureSetup | null) => ReactNode;
  readonly onFinish: () => void;
  readonly onDelete: () => Promise<void>;
}

/** A value, fix, or reference draft's row: its name and what is left to do, the page's cells, and its buttons. */
export function CaptureDraftRow({ resumed, problem, cells, onFinish, onDelete }: CaptureRowProps) {
  const { def } = resumed;
  const fix = def.kind === 'fix';
  const name = fix ? `Fix of ${def.name}` : def.name;
  return (
    <tr className="draft-row">
      <td>
        {name} <DraftPill changes={fix || def.changes !== null} />
        <DraftNote missing={def.missing} problem={problem} />
      </td>
      {cells(keptSetup(def))}
      <td className="end">
        <DraftActions name={name} problem={problem} onFinish={onFinish} onDelete={onDelete} />
      </td>
    </tr>
  );
}
