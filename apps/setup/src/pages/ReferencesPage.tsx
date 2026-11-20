import { useRef, useState } from 'react';

import type { AppSummary, DraftSummary } from '../lib/bindings.ts';
import { deleteReference, errorText } from '../lib/api.ts';
import { andList, readers, referenceCharacters } from '../lib/reference.ts';
import { originText } from '../lib/sharing.ts';
import { CharacterChips } from '../components/CharacterChips.tsx';
import { DeleteActions } from '../components/DeleteActions.tsx';
import type { SavedRef } from '../lib/flow.ts';
import { type Resumed, draftOfSaved, draftProblem, draftsOf } from '../lib/drafts.ts';
import { labeledCharacters } from '../lib/captureDraft.ts';
import { CaptureDraftRow, DraftPill } from '../components/Drafts.tsx';

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onNew: () => void;
  readonly onEdit: (reference: SavedRef) => void;
  /** Every saved draft, or null before they are read. */
  readonly drafts: readonly DraftSummary[] | null;
  readonly onFinish: (resumed: Resumed) => void;
  /** Deletes a draft; the page shows why when it fails. */
  readonly onDeleteDraft: (id: string) => Promise<void>;
}

/** Each application's references: the characters its number values read with. Drafts follow each application's. */
export function ReferencesPage({ apps, loadError, onChanged, onNew, onEdit, drafts, onFinish, onDeleteDraft }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const remove = async (app: string, reference: string): Promise<void> => {
    try {
      await deleteReference(app, reference);
      setDeleteError(null);
      // The question stays until the row is gone, so its buttons take no press meanwhile.
      await onChanged();
      setConfirming((current) => (current === `${app}/${reference}` ? null : current));
    } catch (error: unknown) {
      setDeleteError(errorText(error));
    }
  };

  const headingRef = useRef<HTMLHeadingElement>(null);
  const removeDraft = async (id: string): Promise<void> => {
    try {
      await onDeleteDraft(id);
      setDeleteError(null);
      // The deleted row took the focused button with it.
      headingRef.current?.focus();
    } catch (error: unknown) {
      setDeleteError(errorText(error));
    }
  };
  // Drafts of new references, and of changes to ones no longer saved, which then make a new reference.
  const fresh = draftsOf(drafts, 'reference').filter((resumed) => {
    const file = (apps ?? []).find((app) => app.id === resumed.def.app)?.file ?? null;
    return resumed.def.changes === null || file?.references.every((each) => each.id !== resumed.def.changes) !== false;
  });
  const savedApp = (id: string | null): boolean => (apps ?? []).some((app) => app.id === id && app.file !== null);
  const draftsOfApp = (id: string | null): readonly Resumed[] =>
    fresh.filter((resumed) => (id === null ? !savedApp(resumed.def.app) : resumed.def.app === id));
  const draftRows = (list: readonly Resumed[]) =>
    list.map((resumed) => (
      <CaptureDraftRow
        key={resumed.id}
        resumed={resumed}
        problem={draftProblem(resumed.def, apps ?? [])}
        cells={(setup) => {
          const characters = setup === null ? '' : labeledCharacters(setup);
          return (
            <>
              <td className="note">{characters === '' ? 'No characters yet' : characters}</td>
              <td className="note">Can&apos;t be used until finished</td>
            </>
          );
        }}
        onFinish={() => {
          onFinish(resumed);
        }}
        onDelete={async () => {
          await removeDraft(resumed.id);
        }}
      />
    ));
  const orphans = draftsOfApp(null);
  const withReferences = (apps ?? []).filter(
    (app) => app.file !== null && (app.file.references.length > 0 || draftsOfApp(app.id).length > 0),
  );

  return (
    <main className="page">
      <div className="page-head">
        <h1 ref={headingRef} tabIndex={-1}>
          References
        </h1>
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
      {apps !== null && withReferences.length === 0 && orphans.length === 0 ? (
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
      {withReferences.length === 0 && orphans.length === 0 ? null : (
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
                  const changes = draftOfSaved(drafts, 'reference', reference.id, app.id);
                  return (
                    <tr key={key}>
                      <td>
                        {reference.name}
                        {changes === null ? null : (
                          <>
                            {' '}
                            <DraftPill changes />
                          </>
                        )}
                        {reference.origin.kind === 'byHand' ? null : (
                          <span className="note origin">{originText(reference.origin)}</span>
                        )}
                      </td>
                      <td>
                        <CharacterChips characters={referenceCharacters(reference)} ink={reference.ink} />
                      </td>
                      <td className="note">{used.length === 0 ? 'No values yet' : andList(used)}</td>
                      <td className="end">
                        <DeleteActions
                          name={reference.name}
                          actLabel="Delete reference"
                          title={used.length === 0 ? undefined : `${andList(used)} read with it`}
                          confirming={confirming === key}
                          onConfirm={(asking) => {
                            setConfirming(asking ? key : null);
                          }}
                          onDelete={() => remove(app.id, reference.id)}
                        >
                          {changes === null ? (
                            <button
                              type="button"
                              className="btn small"
                              onClick={() => {
                                onEdit({ app: app.id, reference });
                              }}
                            >
                              Edit<span className="visually-hidden"> {reference.name}</span>
                            </button>
                          ) : (
                            // Its draft holds the changes, so editing goes on from there rather than from the saved one.
                            <button
                              type="button"
                              className="btn small"
                              onClick={() => {
                                onFinish(changes);
                              }}
                            >
                              Finish draft<span className="visually-hidden"> of {reference.name}</span>
                            </button>
                          )}
                        </DeleteActions>
                      </td>
                    </tr>
                  );
                }),
                ...draftRows(draftsOfApp(app.id)),
              ];
            })}
            {orphans.length === 0 ? null : (
              <>
                <tr className="group">
                  <td colSpan={4}>
                    Drafts
                    <small className="warn">Their application is no longer saved, or its file cannot be read.</small>
                  </td>
                </tr>
                {draftRows(orphans)}
              </>
            )}
          </tbody>
        </table>
      )}
    </main>
  );
}
