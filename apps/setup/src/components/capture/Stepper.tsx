import type { Step } from '../../lib/captureDraft.ts';

/** A step of setting up a value, as its tab names it. */
export interface StepTab {
  readonly step: Step;
  readonly name: string;
}

/**
 * The steps a value is set up in: where it shows, then how to read it, which for a number that picks its reference is
 * its characters. The second step reads the live window and saves.
 */
export function stepTabs(picksReference: boolean): readonly StepTab[] {
  return [
    { step: 'where', name: 'Where to read' },
    { step: 'how', name: picksReference ? 'Characters' : 'How to read it' },
  ];
}

/** What the button that goes on from the first step says. */
export function nextFromWhere(picksReference: boolean): string {
  return picksReference ? 'Next: characters' : 'Next: how to read it';
}

interface Props {
  readonly tabs: readonly StepTab[];
  readonly current: Step;
  /** Whether a step other than the current one has what it sets. */
  readonly done: (step: Step) => boolean;
  readonly onStep: (step: Step) => void;
}

/** The value's steps as tabs, numbered, with a tick on each finished one. */
export function Stepper({ tabs, current, done, onStep }: Props) {
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
