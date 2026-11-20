import { type ReactNode, useEffect, useState } from 'react';

import { errorText } from '../lib/api.ts';
import { background } from '../lib/repeat.ts';
import { Question, useQuestion } from './Question.tsx';

interface Props {
  /** The item's name, which the question asks about. */
  readonly name: string;
  /** What else goes with the item, said in the question. */
  readonly loses?: string | undefined;
  /** The label of the answer that deletes, such as `Delete value`. */
  readonly actLabel: string;
  /** Why it cannot be deleted while something uses it, which Delete says in place of the question. */
  readonly blocked?: string | undefined;
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  /**
   * Deletes the item, and rejects with why when it cannot. The answers take no press until it settles, so one
   * answer deletes once.
   */
  readonly onDelete: () => Promise<void>;
  /** The row's other buttons, which show before Delete. */
  readonly children: ReactNode;
}

/**
 * A saved item's row buttons, Delete last, which asks first in the buttons' place. An item in use, or one whose
 * delete failed, says why there instead.
 */
export function DeleteActions({ name, loses, actLabel, blocked, confirming, onConfirm, onDelete, children }: Props) {
  const { control, refs } = useQuestion<HTMLButtonElement>(confirming);
  const [deleting, setDeleting] = useState(false);
  // Why the delete the user answered for failed, until it is dismissed.
  const [refused, setRefused] = useState<string | null>(null);
  const { keep } = refs;
  useEffect(() => {
    // The refusal takes the place of the locked answers, which held the focus.
    if (refused !== null) {
      keep.current?.focus();
    }
  }, [refused, keep]);
  const why = blocked ?? refused;
  if (confirming && why !== null) {
    return (
      <Question
        refs={refs}
        label={`Can't delete ${name}`}
        keepLabel="OK"
        onKeep={() => {
          setRefused(null);
          onConfirm(false);
        }}
      >
        {why}
      </Question>
    );
  }
  if (confirming) {
    const label = `Delete ${name}?`;
    return (
      <Question
        refs={refs}
        label={label}
        keepLabel="Keep"
        actLabel={actLabel}
        busy={deleting}
        onKeep={() => {
          onConfirm(false);
        }}
        onAct={() => {
          setDeleting(true);
          background(
            onDelete()
              .catch((error: unknown) => {
                setRefused(errorText(error));
              })
              .finally(() => {
                setDeleting(false);
              }),
          );
        }}
      >
        {loses === undefined ? label : `${label} ${loses}`}
      </Question>
    );
  }
  return (
    <span className="confirm">
      {children}
      <button
        ref={control}
        type="button"
        className="btn small"
        onClick={() => {
          // A refusal left under another row's question is about a delete long past.
          setRefused(null);
          onConfirm(true);
        }}
      >
        Delete
      </button>
    </span>
  );
}
