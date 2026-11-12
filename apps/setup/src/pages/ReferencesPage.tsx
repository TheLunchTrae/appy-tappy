import { useState } from 'react';

import type { AppSummary } from '../lib/bindings.ts';
import { deleteReference, errorText } from '../lib/api.ts';
import { referenceCoverage } from '../lib/expected.ts';
import { andList, readers, referenceCharacters } from '../lib/reference.ts';
import { background } from '../lib/repeat.ts';
import { CharacterChips, MissingNote } from '../components/CharacterChips.tsx';
import type { SavedRef } from './ReferenceFlow.tsx';

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onNew: () => void;
  readonly onEdit: (reference: SavedRef) => void;
}

/** Each application's references: the characters its number values read with. */
export function ReferencesPage({ apps, loadError, onChanged, onNew, onEdit }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const remove = async (app: string, reference: string): Promise<void> => {
    try {
      await deleteReference(app, reference);
      setDeleteError(null);
      setConfirming(null);
      await onChanged();
    } catch (error: unknown) {
      setDeleteError(errorText(error));
    }
  };

  const withReferences = (apps ?? []).filter((app) => app.file !== null && app.file.references.length > 0);

  return (
    <main className="page">
      <div className="page-head">
        <h1>References</h1>
        <span className="lead">
          How AppyTappy knows each application&apos;s characters. A number value reads its region with one of these.
        </span>
        <div className="end">
          <button type="button" className="btn primary" onClick={onNew}>
            New reference
          </button>
        </div>
      </div>
      {loadError === null ? null : (
        <p className="warn" role="alert">
          {loadError}
        </p>
      )}
      {deleteError === null ? null : (
        <p className="warn" role="alert">
          {deleteError}
        </p>
      )}
      {apps !== null && withReferences.length === 0 ? (
        <div className="empty">
          <h2>No references yet</h2>
          <p>
            A reference is how AppyTappy knows an application&apos;s characters: their color and the shape of each
            digit. Make one here, or from a number value&apos;s region while you set the value up.
          </p>
          <button type="button" className="btn primary" onClick={onNew}>
            New reference
          </button>
        </div>
      ) : null}
      {withReferences.length === 0 ? null : (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">Name</th>
              <th scope="col">Characters</th>
              <th scope="col">Used by</th>
              <th scope="col">
                <span className="note">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {withReferences.map((app) => {
              const { file } = app;
              if (file === null) {
                return null;
              }
              return [
                <tr key={app.id} className="group">
                  <td colSpan={4}>{file.name}</td>
                </tr>,
                ...file.references.map((reference) => {
                  const key = `${app.id}/${reference.id}`;
                  const used = readers(file, reference.id);
                  return (
                    <tr key={key}>
                      <td>{reference.name}</td>
                      <td>
                        <CharacterChips characters={referenceCharacters(reference)} ink={reference.ink} />
                        <MissingNote characters={referenceCoverage(reference).missing} />
                      </td>
                      <td className="note">{used.length === 0 ? 'No values yet' : andList(used)}</td>
                      <td className="end">
                        {confirming === key ? (
                          <span className="confirm">
                            <button
                              type="button"
                              className="btn small"
                              onClick={() => {
                                setConfirming(null);
                              }}
                            >
                              Keep
                            </button>
                            <button
                              type="button"
                              className="btn small danger"
                              onClick={() => {
                                background(remove(app.id, reference.id));
                              }}
                            >
                              Delete {reference.name}
                            </button>
                          </span>
                        ) : (
                          <span className="confirm">
                            <button
                              type="button"
                              className="btn small"
                              onClick={() => {
                                onEdit({ app: app.id, reference });
                              }}
                            >
                              Edit
                            </button>
                            <button
                              type="button"
                              className="btn small"
                              title={used.length === 0 ? undefined : `${andList(used)} read with it`}
                              onClick={() => {
                                setConfirming(key);
                              }}
                            >
                              Delete
                            </button>
                          </span>
                        )}
                      </td>
                    </tr>
                  );
                }),
              ];
            })}
          </tbody>
        </table>
      )}
    </main>
  );
}
