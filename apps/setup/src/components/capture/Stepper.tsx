import type { Step } from '../../lib/captureDraft.ts';

/** A step of setting up a value, as its tab names it. */
export interface StepTab {
  readonly step: Step;
  readonly name: string;
}

/**
 * The steps a value is set up in. A number's characters and its check are steps of their own, so it has three; other
 * kinds read and save in their second step.
 */
export function stepTabs(three: boolean): readonly StepTab[] {
  return three
    ? [
        { step: 'where', name: 'Where to read' },
        { step: 'how', name: 'Characters' },
        { step: 'check', name: 'Check and save' },
      ]
    : [
        { step: 'where', name: 'Where to read' },
        { step: 'how', name: 'How to read it' },
      ];
}

/** What the button that goes on from the first step says. */
export function nextFromWhere(three: boolean): string {
  return three ? 'Next: characters' : 'Next: how to read it';
}

interface Props {
  readonly tabs: readonly StepTab[];
  readonly current: Step;
  /** Whether a step other than the current one has what it sets. */
  readonly done: (step: Step) => boolean;
  /** Whether the steps wait, as they do while a reference is taught. */
  readonly locked: boolean;
  readonly onStep: (step: Step) => void;
}

/** The value's steps as tabs, numbered, with a tick on each finished one. */
export function Stepper({ tabs, current, done, locked, onStep }: Props) {
  return (
    <div className="stepper" role="group" aria-label="Steps">
      {tabs.map((tab, number) => {
        const isCurrent = tab.step === current;
        const finished = !isCurrent && done(tab.step);
        return (
          <button
            key={tab.step}
            type="button"
            className="step-tab"
            aria-current={isCurrent ? 'step' : undefined}
            disabled={locked}
            onClick={() => {
              onStep(tab.step);
            }}
          >
            <span className={finished ? 'num done' : 'num'}>{finished ? '✓' : `${number + 1}`}</span>
            {tab.name}
          </button>
        );
      })}
    </div>
  );
}
