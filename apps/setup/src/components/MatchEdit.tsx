import type { PickerApp } from '../lib/picker.ts';

interface Props {
  readonly app: PickerApp;
  /** Whether the application has a draft of changes, which changing how it is found finishes. */
  readonly drafted: boolean;
  readonly disabled: boolean;
  /** Why the application's editor cannot open from here now, or null when it can. */
  readonly blocked?: string | null;
  readonly onEdit: () => void;
}

/** The way from a window that cannot be told from the application's match to the application's editor. */
export function MatchEdit({ app, drafted, disabled, blocked = null, onEdit }: Props) {
  return (
    <>
      <button
        type="button"
        className="btn"
        data-landing="match"
        disabled={disabled || blocked !== null}
        onClick={onEdit}
      >
        {drafted ? (
          <>
            Finish draft<span className="visually-hidden"> of {app.name}</span>
          </>
        ) : (
          <>Change how it&apos;s found</>
        )}
      </button>
      {blocked !== null ? (
        <p className="note">{blocked}</p>
      ) : drafted ? (
        // Changing it here apart from its draft would leave the two to overwrite each other.
        <p className="note">{app.name} has a draft of changes. To change how it&apos;s found, finish that draft.</p>
      ) : null}
    </>
  );
}
