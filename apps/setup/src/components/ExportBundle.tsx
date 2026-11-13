import { useState } from 'react';

import type { TraceSummary } from '../lib/bindings.ts';
import { debugBundle, errorText } from '../lib/api.ts';
import { Dialog } from './Dialog.tsx';
import { bundleName, sizeText } from '../lib/debug.ts';
import { background } from '../lib/repeat.ts';
import { pickZipFile } from '../lib/zipFile.ts';

interface Props {
  /** The newest trace file, which the bundle can hold, or null without one. */
  readonly trace: TraceSummary | null;
  readonly onClose: () => void;
}

/**
 * Asks what the debug bundle holds, then makes it and saves it where the user picks. The dialog says what the bundle
 * can show, and that nothing is sent anywhere.
 */
export function ExportBundle({ trace, onClose }: Props) {
  const [pictures, setPictures] = useState(false);
  const [withTrace, setWithTrace] = useState(trace !== null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save = async (): Promise<void> => {
    setBusy(true);
    try {
      const write = await pickZipFile(bundleName(new Date()));
      if (write === null) {
        return;
      }
      await write(await debugBundle({ pictures, trace: withTrace && trace !== null ? trace.file : null }));
      onClose();
    } catch (failure: unknown) {
      setError(`${errorText(failure)} The bundle was not saved.`);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog labelledBy="bundle-title" describedBy="bundle-what" busy={busy} onClose={onClose}>
      <h2 id="bundle-title">Export debug bundle</h2>
      <p id="bundle-what">
        Saves one zip file with your settings, automations, applications and both logs, to share when something goes
        wrong. Nothing is sent anywhere.
      </p>
      <div className="check-row">
        <input
          id="bundle-pictures"
          type="checkbox"
          aria-describedby="bundle-pictures-note"
          checked={pictures}
          onChange={(event) => {
            setPictures(event.target.checked);
          }}
        />
        <span className="what">
          <label htmlFor="bundle-pictures">Include pictures</label>
          <span className="note" id="bundle-pictures-note">
            The pictures your values, references and click actions keep, and the screenshots your drafts keep. They show
            parts of your applications&apos; windows, and a draft&apos;s screenshots show whole windows.
          </span>
        </span>
      </div>
      <div className="check-row">
        <input
          id="bundle-trace"
          type="checkbox"
          aria-describedby="bundle-trace-note"
          checked={withTrace && trace !== null}
          disabled={trace === null}
          onChange={(event) => {
            setWithTrace(event.target.checked);
          }}
        />
        <span className="what">
          <label htmlFor="bundle-trace">Include the newest trace</label>
          <span className="note" id="bundle-trace-note">
            {trace === null ? 'There is no trace yet.' : `${trace.file}, ${sizeText(trace.bytes)}`}
          </span>
        </span>
      </div>
      <p className="warn">The bundle holds your applications&apos; window titles and names.</p>
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
          {busy ? 'Saving…' : 'Save bundle'}
        </button>
      </div>
    </Dialog>
  );
}
