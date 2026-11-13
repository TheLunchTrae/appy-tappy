import { CHECK_OFFER_HINT } from '../lib/hints.ts';
import { andList } from '../lib/reference.ts';
import { Hint } from './Hint.tsx';

/** How many characters the offer shows before it counts the rest. */
const SHOWN_CHARACTERS = 12;

/** The id of the offer's text, which Save reference points at so a screen reader reads the offer with it. */
export const CHECK_OFFER_ID = 'check-offer-text';

interface Props {
  /** The characters the reference would learn that nobody has checked. */
  readonly characters: readonly string[];
  readonly onCheck: () => void;
}

/**
 * The offer to check a reference's characters before it is saved, while it would learn some that nobody has
 * checked. It sits above Save reference and never holds the save up: the user takes it, or saves.
 */
export function CheckOffer({ characters, onCheck }: Props) {
  if (characters.length === 0) {
    return null;
  }
  const shown = characters.slice(0, SHOWN_CHARACTERS);
  const more = characters.length - shown.length;
  const one = characters.length === 1;
  return (
    <section className="check-offer" aria-labelledby="check-offer-title">
      <span className="name">
        <span id="check-offer-title">Not checked yet</span>
        <Hint topic="Not checked yet" id="hint-check-offer">
          {CHECK_OFFER_HINT}
        </Hint>
      </span>
      <span
        className="glyphs unchecked"
        role="img"
        aria-label={`${one ? 'Character' : 'Characters'} not checked yet: ${andList([...characters])}`}
      >
        {shown.map((character) => (
          <span key={character} className="glyph" aria-hidden="true">
            {character}
          </span>
        ))}
        {more === 0 ? null : (
          <span className="note" aria-hidden="true">
            and {more} more
          </span>
        )}
      </span>
      <p id={CHECK_OFFER_ID} className="note">
        AppyTappy read {one ? 'this character' : 'these characters'}, and nobody has typed or checked{' '}
        {one ? 'it' : 'them'}. Check {one ? 'it' : 'them'} now, or save the reference as it is.
      </p>
      <div className="row">
        <button type="button" className="btn small" aria-describedby="hint-check-offer" onClick={onCheck}>
          Check characters
        </button>
      </div>
    </section>
  );
}
