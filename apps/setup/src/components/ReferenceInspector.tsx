import type { Dispatch, ReactNode, SetStateAction } from 'react';

import type { AppFile } from '../lib/bindings.ts';
import { explainReading } from '../lib/api.ts';
import { boxOf } from '../lib/boxes.ts';
import type { Editing } from '../lib/captureDraft.ts';
import type { Built, Draft } from '../lib/check.ts';
import { TOO_MANY } from '../lib/expected.ts';
import type { Picture } from '../lib/pixels.ts';
import { readers } from '../lib/reference.ts';
import { background } from '../lib/repeat.ts';
import { referenceSave } from '../lib/teaching.ts';
import type { useBoxes } from '../lib/useBoxes.ts';
import type { LiveCheck } from '../lib/useLiveCheck.ts';
import type { Teaching } from '../lib/useReferenceTeaching.ts';
import { BoxList } from './BoxList.tsx';
import { ReviewActions } from './capture/StageBar.tsx';
import { CharacterCheckSheet } from './CharacterCheck.tsx';
import { CHECK_OFFER_ID, CheckOffer } from './CheckOffer.tsx';
import { ReadingBox } from './Fields.tsx';
import { FoundPanel } from './Found.tsx';
import { ReadingDetailsBox } from './ReadingDetails.tsx';
import { RecordPanel, ReviewSheet } from './Recording.tsx';
import { ReferenceEditor } from './ReferenceEditor.tsx';

/** The id of the note that says why Save reference waits, which the button points at. */
const SAVE_WAITS_ID = 'save-reference-waits';

/** What shows in place of the window's picture while a reference is taught, with its buttons, or null. */
export function teachingSheet(teaching: Teaching): { readonly actions: ReactNode; readonly content: ReactNode } | null {
  const { guided, reviewed, review, recorder, needs } = teaching;
  if (guided.guide !== null) {
    return {
      actions: (
        <button type="button" className="btn" onClick={guided.close}>
          {reviewed !== null ? 'Back to review' : 'Back to the screenshot'}
        </button>
      ),
      content: (
        <CharacterCheckSheet
          guide={guided.guide}
          places={guided.places}
          needs={needs}
          picture={guided.picture}
          onTurn={guided.act}
          onEdit={guided.edit}
          onClose={guided.close}
        />
      ),
    };
  }
  if (reviewed === null) {
    return null;
  }
  return {
    actions: (
      <ReviewActions
        review={review}
        settingsChanged={teaching.settingsChanged}
        recordBlocked={teaching.recordBlocked}
        recordMore={teaching.focus.more}
        onDiscard={() => {
          background(teaching.discard());
        }}
        onUseSettings={teaching.useSettings}
        onRecordMore={() => {
          background(teaching.record(true));
        }}
      />
    ),
    content: (
      <ReviewSheet
        view={reviewed}
        found={teaching.found}
        needs={needs}
        adds={review.adds}
        entries={review.entries}
        problems={review.flagged}
        crops={recorder.crops}
        clashes={review.clashes}
        untaught={review.untaught}
        blocked={review.keepBlocked}
        error={teaching.recordError}
        onEntry={review.edit}
        onKeepAll={review.keepAll}
        onCheck={guided.places.length === 0 ? null : guided.open}
      />
    ),
  };
}

interface Props {
  readonly teaching: Teaching;
  /** The reference the editor teaches. */
  readonly editing: Editing;
  readonly setEditing: Dispatch<SetStateAction<Editing | null>>;
  readonly draft: Draft;
  readonly setDraft: Dispatch<SetStateAction<Draft>>;
  readonly boxes: ReturnType<typeof useBoxes>;
  readonly appName: string;
  /** The application's file, for the values that read with the reference, or null until it is read. */
  readonly file: AppFile | null;
  /** The value's steps, which wait while its reference is taught, or null for a reference made by itself. */
  readonly stepper: ReactNode;
  /** Whether the characters' color is being picked from the picture. */
  readonly pickingInk: boolean;
  readonly canPick: boolean;
  readonly onPickInk: (picking: boolean) => void;
  /** Each screenshot's pixels, and where each sits in the strip. */
  readonly full: ReadonlyMap<number, Picture>;
  readonly position: (index: number) => number;
  readonly setSelected: (index: number | null) => void;
  /** The matching pixels' field. */
  readonly preview: ReactNode;
  /** The characters as they would be saved, or what they still need. */
  readonly built: Built<unknown>;
  /** The try of the characters on the live window. */
  readonly check: LiveCheck;
  readonly busy: boolean;
  readonly draftError: ReactNode;
  readonly draftButton: ReactNode;
  /** Leaves the editor without saving. */
  readonly onDiscard: () => void;
}

/** The capture screen's inspector while a reference is taught: its editor, what it reads now, and its save. */
export function ReferenceInspector({
  teaching,
  editing,
  setEditing,
  draft,
  setDraft,
  boxes,
  appName,
  file,
  stepper,
  pickingInk,
  canPick,
  onPickInk,
  full,
  position,
  setSelected,
  preview,
  built,
  check,
  busy,
  draftError,
  draftButton,
  onDiscard,
}: Props) {
  const { taught, finder, guided, review, recorder, checking, covered, overLimit } = teaching;
  const chosen = boxes.selected;
  const refSave = referenceSave({
    named: editing.name.trim().length > 0,
    built: built.ok,
    tried: check.state.kind === 'set',
    busy,
    overLimit,
    reviewing: teaching.reviewed !== null,
  });
  return (
    <aside className="inspector" aria-label="Reference">
      {stepper}
      <ReferenceEditor
        name={editing.name}
        appName={appName}
        ink={draft.ink}
        tolerance={draft.tolerance}
        picking={pickingInk}
        canPick={canPick}
        saved={taught.saved}
        labeled={taught.labeled}
        recorded={taught.recorded}
        finding={finder.rows.filter((row) => row.keep).map((row) => row.text)}
        expected={editing.expects}
        coverage={covered}
        readers={editing.existing === null || file === null ? [] : readers(file, editing.existing.id)}
        onName={(name) => {
          setEditing((current) => (current === null ? null : { ...current, name }));
        }}
        onPick={() => {
          onPickInk(!pickingInk);
        }}
        onTolerance={(tolerance) => {
          setDraft((current) => ({ ...current, tolerance }));
        }}
        onExpected={(expects) => {
          setEditing((current) => (current === null ? null : { ...current, expects }));
        }}
        boxes={
          <BoxList
            boxes={draft.boxes}
            selected={boxes.selectedId}
            full={full}
            position={position}
            blocked={teaching.boxBlock}
            // A reference taught from a value reads at the value's region, not at a box.
            readsAt={draft.region === null ? (draft.boxes[0]?.id ?? null) : null}
            onSelect={(id) => {
              const box = boxOf(draft.boxes, id);
              boxes.select(id);
              if (box !== null) {
                setSelected(box.screenshot);
              }
            }}
            onText={boxes.retype}
            onDelete={boxes.remove}
          />
        }
        findPanel={
          chosen === null && finder.view === null ? null : (
            <FoundPanel
              finder={finder}
              findBlocked={teaching.findBlocked}
              onCheck={checking ? null : guided.open}
              onFind={() => {
                if (chosen !== null) {
                  onPickInk(false);
                  setSelected(chosen.screenshot);
                  background(finder.search(chosen));
                }
              }}
            />
          )
        }
        preview={preview}
        record={
          <RecordPanel
            view={teaching.recording}
            found={teaching.found}
            needs={teaching.needs.length > 0}
            reviewing={review.reviewing}
            kept={draft.recorded.size}
            keeping={review.keeping.length}
            blocked={teaching.recordBlocked}
            scope={teaching.recordScope}
            onScope={teaching.setRecordScope}
            appName={appName}
            crops={recorder.crops}
            error={teaching.recordError}
            onStart={() => {
              background(teaching.record(false));
            }}
            onMore={() => {
              background(teaching.record(true));
            }}
            onStop={() => {
              background(teaching.stop());
            }}
            onDiscard={() => {
              background(teaching.discard());
            }}
            onReview={() => {
              review.show(true);
            }}
            settingsChanged={teaching.settingsChanged}
            onUseSettings={teaching.useSettings}
            focus={teaching.focus}
          />
        }
      />
      <ReadingBox built={built.ok ? null : built.next} state={check.state} reads={check.reads} />
      {/* Scores are of the characters alone here, so they need no key by the check's kind. */}
      <ReadingDetailsBox ask={check.state.kind === 'set' ? explainReading : null} />
      <div className="sep" />
      {teaching.error === null ? null : <p className="warn">{teaching.error}</p>}
      {overLimit ? <p className="warn">{TOO_MANY}</p> : null}
      {refSave.waits === null ? null : (
        <p id={SAVE_WAITS_ID} className="note">
          {refSave.waits}
        </p>
      )}
      {refSave.can && covered.missing.length > 0 ? (
        <p className="note">It has yet to learn every character it needs. You can save now and teach the rest later.</p>
      ) : null}
      {draftError}
      {checking ? null : <CheckOffer characters={guided.unconfirmed} onCheck={guided.open} />}
      <div className="actions">
        <button type="button" className="btn" disabled={busy} onClick={onDiscard}>
          Discard
        </button>
        {draftButton}
        <button
          type="button"
          className="btn primary"
          disabled={!refSave.can}
          aria-describedby={
            refSave.waits !== null
              ? SAVE_WAITS_ID
              : checking || guided.unconfirmed.length === 0
                ? undefined
                : CHECK_OFFER_ID
          }
          onClick={() => {
            background(teaching.save());
          }}
        >
          Save reference
        </button>
      </div>
    </aside>
  );
}
