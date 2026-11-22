import type { Dispatch, ReactNode, SetStateAction } from 'react';

import type { AppFile } from '../lib/bindings.ts';
import { explainReading } from '../lib/api.ts';
import { boxOf } from '../lib/boxes.ts';
import { type Editing, type NeedsField, savedReads } from '../lib/captureDraft.ts';
import { type Built, type Draft, type ReferenceSaving, withoutSample } from '../lib/check.ts';
import type { Picture } from '../lib/pixels.ts';
import { readers, readersLose, readersNeed } from '../lib/reference.ts';
import { neededPairs } from '../lib/useReadingRisks.ts';
import { background } from '../lib/repeat.ts';
import { checkBackLabel, referenceSave } from '../lib/teaching.ts';
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
import { UnlearnedPictures } from './UnlearnedPictures.tsx';

/** What the reading box says while a saved reference's settings are all there is to save. */
const SETTINGS_ONLY =
  'Saving keeps its samples as they are. Draw a box around some characters on a screenshot to teach it more and see what it reads.';

/** The id of the note that says why Save reference waits, which the button points at. */
const SAVE_WAITS_ID = 'save-reference-waits';

/** What shows in place of the window's picture while a reference is taught, with its buttons, or null. */
export function teachingSheet(teaching: Teaching): { readonly actions: ReactNode; readonly content: ReactNode } | null {
  const { guided, reviewed, review, recorder, needs } = teaching;
  if (guided.guide !== null) {
    return {
      actions: (
        <button type="button" className="btn" onClick={guided.close}>
          {checkBackLabel(reviewed !== null)}
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
        readings={reviewed.readings.length}
        onDiscard={teaching.discard}
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
  /** Each screenshot's pixels, and where each sits in the strip. */
  readonly full: ReadonlyMap<number, Picture>;
  readonly position: (index: number) => number;
  readonly setSelected: (index: number | null) => void;
  /** What saving the reference sends, or what it still needs. */
  readonly saving: Built<ReferenceSaving>;
  /** The try of the characters on the live window. */
  readonly check: LiveCheck;
  /** Why there is no live window to read, or null while one is captured. */
  readonly windowless: string | null;
  readonly busy: boolean;
  readonly draftError: ReactNode;
  readonly draftButton: ReactNode;
  /** How the editor shows the characters the value it is taught for needs. */
  readonly needsField: NeedsField;
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
  full,
  position,
  setSelected,
  saving,
  check,
  windowless,
  busy,
  draftError,
  draftButton,
  needsField,
  onDiscard,
}: Props) {
  const { taught, finder, guided, review, recorder, checking, covered } = teaching;
  const chosen = boxes.selected;
  const saved = editing.existing?.samples ?? [];
  // A look-alike pair warns when it has one of these characters: the value's, or the ones the reference reads, or for
  // a reference by itself that reads every character it learns, those of every value that reads with it.
  const lookalikeNeeds =
    needsField !== 'hidden' || teaching.needs.length > 0
      ? teaching.needs
      : editing.existing === null || file === null
        ? []
        : readersNeed(file, editing.existing.id);
  const settingsOnly = saving.ok && saving.spec.kind === 'settings';
  const refSave = referenceSave({
    named: editing.name.trim().length > 0,
    built: saving.ok,
    // Settings alone teach nothing to try: the reference reads as it is saved.
    tried: settingsOnly || check.state.kind === 'set',
    busy,
    reviewing: teaching.reviewed !== null,
    unlearned: teaching.unlearned.samples.length,
  });
  return (
    <aside className="inspector" aria-label="Reference">
      {stepper}
      <ReferenceEditor
        name={editing.name}
        appName={appName}
        saved={taught.saved}
        labeled={taught.labeled}
        recorded={taught.recorded}
        finding={finder.rows.filter((row) => row.keep).map((row) => row.text)}
        reads={editing.reads}
        readersLose={
          editing.existing === null || file === null
            ? null
            : readersLose(file, editing.existing.id, savedReads(editing))
        }
        onReads={(reads) => {
          setEditing((current) => (current === null ? null : { ...current, reads }));
        }}
        valueNeeds={editing.needs}
        expected={teaching.needs}
        coverage={covered}
        lookalikes={neededPairs(teaching.risks.lookalikes, lookalikeNeeds)}
        risksProblem={teaching.risks.problem}
        readers={editing.existing === null || file === null ? [] : readers(file, editing.existing.id)}
        onName={(name) => {
          setEditing((current) => (current === null ? null : { ...current, name }));
        }}
        needsField={needsField}
        onExpected={(needs) => {
          setEditing((current) => (current === null ? null : { ...current, needs }));
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
                  setSelected(chosen.screenshot);
                  background(finder.search(chosen));
                }
              }}
            />
          )
        }
        unlearned={
          <UnlearnedPictures
            unlearned={teaching.unlearned}
            removed={saved.length - taught.saved.length}
            onRemove={(image) => {
              setDraft((current) => withoutSample(current, image));
            }}
            onPutBack={() => {
              setDraft((current) => ({ ...current, dropped: [] }));
            }}
          />
        }
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
            onDiscard={teaching.discard}
            onReview={() => {
              review.show(true);
            }}
            settingsChanged={teaching.settingsChanged}
            onUseSettings={teaching.useSettings}
            focus={teaching.focus}
          />
        }
      />
      <ReadingBox
        built={settingsOnly ? SETTINGS_ONLY : saving.ok ? null : saving.next}
        state={check.state}
        reads={check.reads}
        windowless={windowless}
      />
      {/* Scores are of the characters alone here, so they need no key by the check's kind. */}
      <ReadingDetailsBox ask={check.state.kind === 'set' ? explainReading : null} />
      <div className="sep" />
      {teaching.error === null ? null : (
        <p className="warn" role="alert">
          {teaching.error}
        </p>
      )}
      {refSave.waits === null ? null : (
        <p id={SAVE_WAITS_ID} className="note">
          {refSave.waits}
        </p>
      )}
      {refSave.can && covered.missing.length > 0 ? (
        <p className="note">
          It has yet to learn every character {editing.needs.length > 0 ? 'the value needs' : 'it reads'}. You can save
          now and teach the rest later.
        </p>
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
