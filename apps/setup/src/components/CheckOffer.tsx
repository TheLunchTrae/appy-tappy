import { CHECK_OFFER_HINT } from '../lib/hints.ts';
import { andList } from '../lib/reference.ts';
import { Hint } from './Hint.tsx';

/** How many characters the offer shows before it counts the rest. */
const SHOWN_CHARACTERS = 12;

/** The id of the offer's text, which Save reference points at so a screen reader reads the offer with it. */
export const CHECK_OFFER_ID = 'check-offer-text';

interface Props {
  /** The characters the reference would learn that nobody has confirmed. */
  readonly characters: readonly string[];
  readonly onCheck: () => void;
}

/**
 * The offer to check a reference's characters before it is saved, while it would learn some that are the text
 * reader's guesses and that nobody has confirmed one at a time. It sits above Save reference and never holds the
 * save up: the user takes it, or saves.
 *
 * It says "confirmed", not "checked": the recording panel's Found, not checked yet counts a reading as checked once
 * the user keeps it, and the two show together.
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
        <span id="check-offer-title">Not confirmed yet</span>
        <Hint topic="Not confirmed yet" id="hint-check-offer">
          {CHECK_OFFER_HINT}
        </Hint>
      </span>
      <span
        className="glyphs unchecked"
        role="img"
        aria-label={`${one ? 'Character' : 'Characters'} not confirmed yet: ${andList([...characters])}`}
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
        {one
          ? "This is AppyTappy's guess: it read the character, and nobody has typed it or confirmed it. Confirm it with Check characters, or save the reference as it is."
          : "These are AppyTappy's guesses: it read them, and nobody has typed them or confirmed them one at a time. Confirm them with Check characters, or save the reference as it is."}
      </p>
      <div className="row">
        <button type="button" className="btn small" aria-describedby="hint-check-offer" onClick={onCheck}>
          Check characters
        </button>
      </div>
    </section>
  );
}
