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
 * Focus for a question that takes a control's place while `showing`: it moves to the question's safe answer when
 * the question shows, and back to the control after an answer given in the question. A question that shows by
 * itself, with `asked` false, leaves the focus where the user has it. Put `control` on the control and pass `refs`
 * to the {@link Question}.
 */
export function useQuestion<T extends HTMLElement>(showing: boolean, asked = showing): QuestionFocus<T> {
  const control = useRef<T>(null);
  const keep = useRef<HTMLButtonElement>(null);
  const question = useRef<HTMLSpanElement>(null);
  const hadFocus = useRef(false);
  useEffect(() => {
    if (showing) {
      if (asked) {
        keep.current?.focus();
      }
    } else if (hadFocus.current) {
      hadFocus.current = false;
      control.current?.focus();
    }
  }, [showing, asked]);
  const refs: QuestionRefs = { keep, question, hadFocus };
  return { control, refs };
}

interface Shared {
  readonly refs: QuestionRefs;
  /** The question itself, which names it for screen readers, such as `Remove Example?`. */
  readonly label: string;
  /** The safe answer's label; its button takes focus. */
  readonly keepLabel: string;
  /** Whether the answer is under way, which locks the buttons. */
  readonly busy?: boolean;
  readonly onKeep: () => void;
  /** A third answer between the two, the one to pick most often, such as keeping the work for later. */
  readonly middle?: { readonly label: string; readonly onPress: () => void } | undefined;
  /** The warning, read with the question. */
  readonly children: ReactNode;
}

/** The other answer, whose button does what the question asks about. */
interface Asks {
  readonly actLabel: string;
  readonly onAct: () => void;
}

/** No other answer: the warning says why the control does nothing, and the safe answer dismisses it. */
interface Tells {
  readonly actLabel?: undefined;
  readonly onAct?: undefined;
}

type Props = Shared & (Asks | Tells);

/**
 * A question in a control's place: a warning and two answers, the safe one first, or three with `middle` between
 * them. Without the other answer it only tells: the warning and the safe answer. Escape is the same as keep.
 */
export function Question({ refs, label, keepLabel, actLabel, busy = false, onKeep, onAct, middle, children }: Props) {
  const warning = useId();
  const answer = (act: (() => void) | boolean): void => {
    refs.hadFocus.current = refs.question.current?.contains(document.activeElement) ?? false;
    if (typeof act === 'function') {
      act();
    } else if (act) {
      onAct?.();
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
        {middle === undefined ? null : (
          <button
            type="button"
            className="btn small primary"
            disabled={busy}
            onKeyDown={escape}
            onClick={() => {
              answer(middle.onPress);
            }}
          >
            {middle.label}
          </button>
        )}
        {actLabel === undefined ? null : (
          <button
            type="button"
            className="btn small danger"
            disabled={busy}
            onKeyDown={escape}
            onClick={(event) => {
              // The question may show under the pointer, where the control was pressed. The later clicks of a
              // double-click there are that press carried on, not an answer.
              if (event.detail <= 1) {
                answer(true);
              }
            }}
          >
            {actLabel}
          </button>
        )}
      </span>
    </span>
  );
}
