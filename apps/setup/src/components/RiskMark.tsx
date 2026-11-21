import type { AppFile, ReferenceDef } from '../lib/bindings.ts';
import { Hint } from './Hint.tsx';
import { readersNeed } from '../lib/reference.ts';
import { useSavedRiskMark } from '../lib/useReadingRisks.ts';

interface Props {
  /** The application's ID. */
  readonly app: string;
  readonly file: AppFile;
  readonly reference: ReferenceDef;
}

/**
 * Marks a saved reference on the References page that may not read as its samples do. The ? beside the mark says
 * why, to the pointer and the keyboard alike.
 */
export function RiskMark({ app, file, reference }: Props) {
  const mark = useSavedRiskMark(app, reference, readersNeed(file, reference.id));
  if (mark === null) {
    return null;
  }
  return (
    <>
      {' '}
      <span className="risk-mark">
        <span className="pill risk">May misread</span>
        <Hint topic="May misread">{mark}</Hint>
      </span>
    </>
  );
}
