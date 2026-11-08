import { type ReactNode, useEffect, useId, useState } from 'react';

interface Props {
  /** What the hint explains, which names its button for screen readers, such as `Window class`. */
  readonly topic: string;
  /** The id the explained control points its `aria-describedby` at, so a screen reader reads the hint with it. */
  readonly id?: string;
  readonly children: ReactNode;
}

/**
 * A ? button that shows a short explanation while it is hovered or focused. The explanation stays while the
 * pointer moves onto it, and Escape hides it.
 */
export function Hint({ topic, id, children }: Props) {
  const own = useId();
  const tipId = id ?? own;
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const open = (hovered || focused) && !dismissed;
  // Escape hides it wherever focus is, without moving the pointer or focus.
  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const escape = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        setDismissed(true);
      }
    };
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('keydown', escape);
    };
  }, [open]);
  return (
    <span
      className="hint-wrap"
      onMouseEnter={() => {
        setHovered(true);
        setDismissed(false);
      }}
      onMouseLeave={() => {
        setHovered(false);
      }}
    >
      <button
        type="button"
        className="hint-btn"
        aria-label={`About ${topic}`}
        aria-describedby={tipId}
        onFocus={() => {
          setFocused(true);
          setDismissed(false);
        }}
        onBlur={() => {
          setFocused(false);
        }}
      >
        <span aria-hidden="true">?</span>
      </button>
      <span id={tipId} role="tooltip" className="tip" hidden={!open}>
        {children}
      </span>
    </span>
  );
}
