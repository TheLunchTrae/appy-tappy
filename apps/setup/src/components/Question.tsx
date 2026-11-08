import { type KeyboardEvent, type ReactNode, type RefObject, useEffect, useId, useRef } from 'react';

/** The parts of a question that move focus, shared by the question and the control it replaces. */
interface QuestionRefs {
  readonly keep: RefObject<HTMLButtonElement | null>;
  readonly question: RefObject<HTMLSpanElement | null>;
  /** Whether focus was in the question when it went, so it comes back to the control only then. */
  readonly hadFocus: RefObject<boolean>;
}

/**
 * Focus for a question that takes a control's place while `confirming`: it moves to the question's safe answer
 * when the question shows, and back to the control after an answer given in the question. Put `control` on the
 * control and pass `refs` to the {@link Question}.
 */
export function useQuestion<T extends HTMLElement>(confirming: boolean) {
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
  /** The safe answer's button, which takes focus. */
  readonly keepLabel: string;
  /** The other answer's button, which does what the question asks about. */
  readonly actLabel: string;
  /** Whether the answer is under way, which locks both buttons. */
  readonly busy?: boolean;
  /** Called with true for the act and false for keep. */
  readonly onAnswer: (act: boolean) => void;
  /** What the act costs, read with the question. */
  readonly children: ReactNode;
}

/** A question in a control's place: a warning and two answers, the safe one first. Escape is the same as keep. */
export function Question({ refs, label, keepLabel, actLabel, busy = false, onAnswer, children }: Props) {
  const warning = useId();
  const answer = (act: boolean): void => {
    refs.hadFocus.current = refs.question.current?.contains(document.activeElement) ?? false;
    onAnswer(act);
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
