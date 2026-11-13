// Work a component started that the owner names by its ticket's number, such as a capture or a recording.

/**
 * The work a component started, by the number the owner gave it, which the component stops by that number: the owner
 * drops a stop of work that other work took the place of, so a component that goes never stops what the next one
 * started.
 */
export function ownTicket(stop: (ticket: number) => void): {
  /** The component shows. */
  readonly enter: () => void;
  /** Hears that the component's start answered `ticket`, which stops at once when the component went meanwhile. */
  readonly started: (ticket: number) => void;
  /** The number of the component's work, or null without any. */
  readonly held: () => number | null;
  /** Stops the component's work, if it has some. */
  readonly stop: () => void;
  /** The component goes, and its work with it. */
  readonly leave: () => void;
} {
  let held: number | null = null;
  let shows = false;
  const end = (): void => {
    if (held !== null) {
      stop(held);
      held = null;
    }
  };
  return {
    enter: () => {
      shows = true;
    },
    started: (ticket) => {
      // The owner replaced the component's last work when it started this one, or went on with it under the same
      // number. The newest has the highest number, whichever start answers last.
      held = Math.max(held ?? ticket, ticket);
      if (!shows) {
        end();
      }
    },
    held: () => held,
    stop: end,
    leave: () => {
      shows = false;
      end();
    },
  };
}
