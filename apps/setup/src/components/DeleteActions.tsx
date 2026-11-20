import type { ReactNode } from 'react';

import { Question, useQuestion } from './Question.tsx';

interface Props {
  /** The item's name, which the question asks about. */
  readonly name: string;
  /** What else goes with the item, said in the question. */
  readonly loses?: string | undefined;
  /** The label of the answer that deletes, such as `Delete value`. */
  readonly actLabel: string;
  /** Delete's tooltip. */
  readonly title?: string | undefined;
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  readonly onDelete: () => void;
  /** The row's other buttons, which show before Delete. */
  readonly children: ReactNode;
}

/** A saved item's row buttons, Delete last, which asks first in the buttons' place. */
export function DeleteActions({ name, loses, actLabel, title, confirming, onConfirm, onDelete, children }: Props) {
  const { control, refs } = useQuestion<HTMLButtonElement>(confirming);
  if (confirming) {
    const label = `Delete ${name}?`;
    return (
      <Question
        refs={refs}
        label={label}
        keepLabel="Keep"
        actLabel={actLabel}
        onKeep={() => {
          onConfirm(false);
        }}
        onAct={onDelete}
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
        title={title}
        onClick={() => {
          onConfirm(true);
        }}
      >
        Delete
      </button>
    </span>
  );
}
