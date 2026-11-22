import type { ReferenceDef } from '../../lib/bindings.ts';
import { READ_WITH_HINT } from '../../lib/sizes.ts';
import { Hint } from '../Hint.tsx';

interface Props {
  /** The application's saved references, or null until its file is read. */
  readonly references: readonly ReferenceDef[] | null;
  /** The reference the value reads with at every size. */
  readonly shared: string;
  /** The reference the fix reads with now. */
  readonly selected: string | null;
  readonly onSelect: (reference: string) => void;
}

/**
 * What a fix of a number reads with: the reference its value shares, or another of the application's saved
 * references, such as one taught on the References page from screenshots at this size.
 */
export function ReadWith({ references, shared, selected, onSelect }: Props) {
  // The shared reference comes first, and stays a choice while the file is read.
  const sharedDef = references?.find((saved) => saved.id === shared);
  const others = (references ?? []).filter((saved) => saved.id !== shared);
  return (
    <fieldset className="choices" aria-describedby="hint-read-with">
      <legend className="group-title">
        <span>Read with</span>
        <Hint topic="Read with" id="hint-read-with">
          {READ_WITH_HINT}
        </Hint>
      </legend>
      <label className="choice" aria-label={sharedDef?.name ?? 'Its reference'}>
        <input
          type="radio"
          name="read-with"
          aria-describedby="read-with-shared"
          checked={selected === shared}
          onChange={() => {
            onSelect(shared);
          }}
        />
        <span>
          <span className="choice-name">{sharedDef?.name ?? 'Its reference'}</span>
          <small id="read-with-shared" className="note">
            The reference it reads with at every size.
          </small>
        </span>
      </label>
      {others.map((reference) => (
        <label key={reference.id} className="choice" aria-label={reference.name}>
          <input
            type="radio"
            name="read-with"
            checked={selected === reference.id}
            onChange={() => {
              onSelect(reference.id);
            }}
          />
          <span>
            <span className="choice-name">{reference.name}</span>
          </span>
        </label>
      ))}
    </fieldset>
  );
}
