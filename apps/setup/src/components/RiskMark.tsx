import type { AppFile, ReferenceDef } from '../lib/bindings.ts';
import { readersNeed } from '../lib/reference.ts';
import { useSavedRiskMark } from '../lib/useReadingRisks.ts';

interface Props {
  /** The application's ID. */
  readonly app: string;
  readonly file: AppFile;
  readonly reference: ReferenceDef;
}

/** Marks a saved reference on the References page that may not read as its samples do, and says why. */
export function RiskMark({ app, file, reference }: Props) {
  const mark = useSavedRiskMark(app, reference, readersNeed(file, reference.id));
  if (mark === null) {
    return null;
  }
  return (
    <>
      {' '}
      <span className="pill risk" title={mark}>
        May misread<span className="visually-hidden">: {mark}</span>
      </span>
    </>
  );
}
