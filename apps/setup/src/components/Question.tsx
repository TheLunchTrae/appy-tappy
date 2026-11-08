import { type KeyboardEvent, type ReactNode, type RefObject, useEffect, useId, useRef } from 'react';

/**
 * The question's own elements and the flag it records for the control it replaces. A cell passes them to its
 * {@link Question} as they are.
 */
interface QuestionRefs {
  readonly keep: RefObject<HTMLButtonElement | null>;
  readonly question: RefObject<HTMLSpanElement | null>;
  /** Whether focus was in the question when it went, so it comes back to the control only then. */
  readonly hadFocus: RefObject<boolean>;
}

/** What {@link useQuestion} gives a cell. */
export interface QuestionFocus<T extends HTMLElement> {
  /** For the control the question replaces, which takes focus back after an answer given in the question. */
  readonly control: RefObject<T | null>;
  readonly refs: QuestionRefs;
}

/**
 * Focus for a question that takes a control's place while `confirming`: it moves to the question's safe answer
 * when the question shows, and back to the control after an answer given in the question. Put `control` on the
 * control and pass `refs` to the {@link Question}.
 */
export function useQuestion<T extends HTMLElement>(confirming: boolean): QuestionFocus<T> {
  const control = useRef<T>(null);
  const keep = useRef<HTMLButtonElement>(null);
  const question = useRef<HTMLSpanElement>(null);
  const hadFocus = useRef(false);
  useEffect(() => {
    if (confirming) {
      keep.current?.focus();
    } else if (hadFocus.current) {
      hadFocus.current = false;
      control.current?.focus();
    }
  }, [confirming]);
  const refs: QuestionRefs = { keep, question, hadFocus };
  return { control, refs };
}

interface Props {
  readonly refs: QuestionRefs;
  /** The question itself, which names it for screen readers, such as `Remove Example?`. */
  readonly label: string;
  /** The safe answer's label; its button takes focus. */
  readonly keepLabel: string;
  /** The other answer's label; its button does what the question asks about. */
  readonly actLabel: string;
  /** Whether the answer is under way, which locks both buttons. */
  readonly busy?: boolean;
  readonly onKeep: () => void;
  readonly onAct: () => void;
  /** The warning, read with the question. */
  readonly children: ReactNode;
}

/** A question in a control's place: a warning and two answers, the safe one first. Escape is the same as keep. */
export function Question({ refs, label, keepLabel, actLabel, busy = false, onKeep, onAct, children }: Props) {
  const warning = useId();
  const answer = (act: boolean): void => {
    refs.hadFocus.current = refs.question.current?.contains(document.activeElement) ?? false;
    if (act) {
      onAct();
    } else {
      onKeep();
    }
  };
  const escape = (event: KeyboardEvent): void => {
    if (event.key === 'Escape') {
      event.preventDefault();
      answer(false);
    }
  };
  return (
    <span ref={refs.question} className="confirm-none" role="alertdialog" aria-label={label} aria-describedby={warning}>
      <span id={warning} className="warn">
        {children}
      </span>
      <span className="confirm">
        <button
          ref={refs.keep}
          type="button"
          className="btn small"
          disabled={busy}
          onKeyDown={escape}
          onClick={() => {
            answer(false);
          }}
        >
          {keepLabel}
        </button>
        <button
          type="button"
          className="btn small danger"
          disabled={busy}
          onKeyDown={escape}
          onClick={() => {
            answer(true);
          }}
        >
          {actLabel}
        </button>
      </span>
    </span>
  );
}
