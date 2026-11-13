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
  readonly onShared: () => void;
  /** Opens the editor on a new reference for this size, which starts with what `like` needs. */
  readonly onLearn: (like: ReferenceDef | null) => void;
}

/** What a fix of a number reads with: the reference its value shares, or one learned at this size. */
export function ReadWith({ references, shared, selected, onShared, onLearn }: Props) {
  const named = (id: string | null): ReferenceDef | null => references?.find((saved) => saved.id === id) ?? null;
  return (
    <fieldset className="choices" aria-describedby="hint-read-with">
      <legend className="group-title">
        <span>Read with</span>
        <Hint topic="Read with" id="hint-read-with">
          {READ_WITH_HINT}
        </Hint>
      </legend>
      <label className="choice" aria-label="Shared reference">
        <input
          type="radio"
          name="read-with"
          aria-describedby="read-with-shared"
          checked={selected === shared}
          onChange={onShared}
        />
        <span>
          <span className="choice-name">{named(shared)?.name ?? 'Its reference'}</span>
          <small id="read-with-shared" className="note">
            The reference it reads with at every size.
          </small>
        </span>
      </label>
      <label className="choice" aria-label="Learn a reference at this size">
        <input
          type="radio"
          name="read-with"
          aria-describedby="read-with-own"
          checked={selected !== shared}
          onChange={() => {
            onLearn(named(shared));
          }}
        />
        <span>
          <span className="choice-name">
            {selected === shared
              ? 'Learn a reference at this size'
              : (named(selected)?.name ?? 'A reference learned at this size')}
          </span>
          <small id="read-with-own" className="note">
            Teach the characters again from this size&apos;s screenshots.
          </small>
        </span>
      </label>
    </fieldset>
  );
}
