import type { AntiCheatClass } from '../lib/bindings.ts';
import { ANTI_CHEAT_CHOICES, ANTI_CHEAT_NAMES } from '../lib/antiCheat.ts';
import { Question, useQuestion } from './Question.tsx';

interface Props {
  /** The application's name, which the question about None says. */
  readonly name: string;
  readonly antiCheat: AntiCheatClass;
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  readonly onSet: (antiCheat: AntiCheatClass) => void;
  /** Whether a change is saving, which locks the question's answers. */
  readonly busy: boolean;
  /** The menu's ID, for a label beside it. */
  readonly id?: string;
  /** What a screen reader calls the menu when no label names it. */
  readonly label?: string;
}

/**
 * An application's anti-cheat, picked from a menu. None lets automations press keys into it and click in it, so
 * picking None asks first, in the menu's place.
 */
export function AntiCheatMenu({ name, antiCheat, confirming, onConfirm, onSet, busy, id, label }: Props) {
  const { control, refs } = useQuestion<HTMLSelectElement>(confirming);
  if (confirming) {
    return (
      <Question
        refs={refs}
        label={`Set ${name}'s anti-cheat to None?`}
        keepLabel={`Keep ${ANTI_CHEAT_NAMES[antiCheat]}`}
        actLabel="Set to None"
        busy={busy}
        onKeep={() => {
          onConfirm(false);
        }}
        onAct={() => {
          onSet('none');
        }}
      >
        Only pick None if {name} has no anti-cheat. Pressing keys into an application with anti-cheat, or clicking in
        one, can get your account banned.
      </Question>
    );
  }
  return (
    <select
      ref={control}
      id={id}
      className="input"
      aria-label={label}
      value={antiCheat}
      onChange={(event) => {
        const picked = ANTI_CHEAT_CHOICES.find((choice) => choice === event.target.value);
        if (picked === 'none') {
          onConfirm(true);
        } else if (picked !== undefined) {
          onSet(picked);
        }
      }}
    >
      {ANTI_CHEAT_CHOICES.map((choice) => (
        <option key={choice} value={choice}>
          {ANTI_CHEAT_NAMES[choice]}
        </option>
      ))}
    </select>
  );
}
