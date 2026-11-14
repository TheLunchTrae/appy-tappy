import { useState } from 'react';

import { errorText, exportApp } from '../lib/api.ts';
import { background } from '../lib/repeat.ts';
import { exportName } from '../lib/sharing.ts';
import { saveZip } from '../lib/zipFile.ts';
import { Dialog } from './Dialog.tsx';

interface Props {
  /** The application's ID. */
  readonly app: string;
  readonly name: string;
  readonly onClose: () => void;
}

/**
 * Says what an exported application's file holds and shows, then makes it and saves it where the user picks, asking
 * where only once it is made.
 * Nothing is sent anywhere.
 */
export function ExportApp({ app, name, onClose }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save = async (): Promise<void> => {
    setBusy(true);
    try {
      if (await saveZip(exportName(name), async () => exportApp(app))) {
        onClose();
      }
    } catch (failure: unknown) {
      setError(`${errorText(failure)} The file was not saved.`);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog titleId="export-title" describedBy="export-what" busy={busy} onClose={onClose}>
      <h2 id="export-title">Export {name}</h2>
      <p id="export-what">
        Saves one zip file with how AppyTappy finds {name}, its values and references, and the pictures they use, for
        someone else to import. Nothing is sent anywhere.
      </p>
      <p className="note">Your automations and the overlay setting stay out of the file.</p>
      <p className="warn">
        The pictures show parts of {name}&apos;s window, and the file holds the window title AppyTappy looks for.
      </p>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      <div className="dialog-actions">
        <button type="button" className="btn" disabled={busy} onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          className="btn primary"
          aria-busy={busy}
          disabled={busy}
          onClick={() => {
            background(save());
          }}
        >
          {busy ? 'Saving…' : 'Save file'}
        </button>
      </div>
    </Dialog>
  );
}
