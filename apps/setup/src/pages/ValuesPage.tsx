import { useRef, useState } from 'react';

import {
  type AppSummary,
  type AutomationSummary,
  type DraftSummary,
  NO_STATE,
  type SizeDef,
  type ValueDef,
  type ValueView,
} from '../lib/bindings.ts';
import { deleteValue, errorText, markChecked, removeFix } from '../lib/api.ts';
import { readingText, usedBy } from '../lib/automation.ts';
import { buildState, checkedText } from '../lib/build.ts';
import { Hint } from '../components/Hint.tsx';
import { MARK_CHECKED_HINT } from '../lib/hints.ts';
import { hex } from '../lib/pixels.ts';
import { background } from '../lib/repeat.ts';
import {
  FIX_HINT,
  NOW_HINT,
  type NowCell,
  nowCell,
  offersFix,
  REMOVE_FIX_HINT,
  regionText,
  sameSize,
  sizeText,
} from '../lib/sizes.ts';
import { type LiveApp, useLiveValues } from '../lib/useLiveValues.ts';
import type { Changing } from '../lib/flow.ts';
import { type Resumed, draftsOf } from '../lib/drafts.ts';
import { captureProblem } from '../lib/captureDraft.ts';
import { CHECKS, checkSummary } from '../lib/check.ts';
import { CaptureDraftRow } from '../components/Drafts.tsx';

/** What a page's draft rows need: every draft, the saved applications, and what Finish and Delete do. */
interface DraftRows {
  readonly apps: readonly AppSummary[];
  readonly onFinish: (resumed: Resumed) => void;
  readonly onDelete: (id: string) => Promise<void>;
}

/** The value and fix drafts, the first belonging to `app`, or with null to no application that is still listed. */
function valueDrafts(drafts: readonly DraftSummary[] | null, apps: readonly AppSummary[], app: string | null) {
  const saved = (id: string | null): boolean => apps.some((each) => each.id === id);
  return [...draftsOf(drafts, 'value'), ...draftsOf(drafts, 'fix')].filter((resumed) =>
    app === null ? !saved(resumed.def.app) : resumed.def.app === app,
  );
}

/** A value or fix draft's rows, with as many cells as the table has columns. */
function ValueDraftRows({
  list,
  nowColumn,
  rows,
}: {
  readonly list: readonly Resumed[];
  readonly nowColumn: boolean;
  readonly rows: DraftRows;
}) {
  return list.map((resumed) => (
    <CaptureDraftRow
      key={resumed.id}
      resumed={resumed}
      problem={captureProblem(resumed.def, rows.apps)}
      cells={(setup) => (
        <>
          <td className="note">{setup === null ? '' : CHECKS[setup.draft.kind].label}</td>
          <td className="note">{setup?.draft.region === null || setup === null ? 'No region yet' : 'Region drawn'}</td>
          {nowColumn ? <td className="note">Doesn&apos;t read</td> : null}
          <td className="note">Can&apos;t be used until finished</td>
        </>
      )}
      onFinish={() => {
        rows.onFinish(resumed);
      }}
      onDelete={async () => {
        await rows.onDelete(resumed.id);
      }}
    />
  ));
}

interface AppRowsProps {
  readonly app: AppSummary;
  readonly automations: readonly AutomationSummary[] | null;
  /** What its values read now, while the page shows a column for that. */
  readonly live: LiveApp | null;
  /** Whether the page shows a column for what values read now. */
  readonly nowColumn: boolean;
  readonly confirming: string | null;
  readonly onConfirm: (key: string | null) => void;
  readonly onDelete: (value: string) => void;
  readonly onMarkChecked: () => void;
  readonly onFix: (value: ValueDef) => void;
  readonly onEdit: (value: ValueDef) => void;
  readonly onRemoveFix: (value: string, size: SizeDef) => void;
  /** Its value and fix drafts. */
  readonly drafts: readonly Resumed[];
  readonly rows: DraftRows;
}

/** What a value's cell for the window's size now says, in a few words. */
function nowText(cell: NowCell, size: SizeDef): string {
  switch (cell.kind) {
    case 'reading':
      return cell.text;
    case 'waiting':
      return 'Reading…';
    case 'noState':
      return 'Matches no state';
    case 'unread':
      return "Doesn't read";
    case 'notDrawn':
      return `Not read at ${sizeText(size)}, the window's size now`;
  }
}

interface NowProps {
  readonly value: ValueDef;
  readonly cell: NowCell;
  readonly size: SizeDef;
  readonly onFix: () => void;
  readonly onRemoveFix: () => void;
}

/** A value's reading at the window's size now, or why it has none, with a way to fix it at that size. A state
 * check that matches none of its states may be right, so it shows as a note, not a warning. */
function NowCellView({ value, cell, size, onFix, onRemoveFix }: NowProps) {
  const tone = { reading: undefined, waiting: undefined, noState: 'note', unread: 'warn', notDrawn: 'warn' }[cell.kind];
  const fixed = cell.kind !== 'notDrawn' && cell.kind !== 'waiting' && cell.from === 'fixed';
  return (
    <span className="now">
      <span className={tone} title={cell.kind === 'unread' ? cell.problem : undefined}>
        {nowText(cell, size)}
      </span>
      {fixed ? <small className="note">fixed at this size</small> : null}
      {offersFix(cell) ? (
        <>
          <button type="button" className="btn small" onClick={onFix}>
            Fix at this size<span className="visually-hidden"> for {value.name}</span>
          </button>
          <Hint topic="Fix at this size">{FIX_HINT}</Hint>
        </>
      ) : null}
      {fixed ? (
        <>
          <button type="button" className="btn small" onClick={onRemoveFix}>
            Remove fix<span className="visually-hidden"> of {value.name}</span>
          </button>
          <Hint topic="Remove fix">{REMOVE_FIX_HINT}</Hint>
        </>
      ) : null}
    </span>
  );
}

/** The reading of `view`, in a few words, and whether it is a state check that matched none of its states. */
function readingOf(view: ValueView | undefined): { readonly text: string; readonly noState: boolean } | null {
  if (view?.reading === undefined || view.reading === null) {
    return null;
  }
  const { reading } = view;
  return { text: readingText(reading), noState: reading.kind === 'state' && reading.value === NO_STATE };
}

/** The automations that use a value, in a few words. */
function usedByText(names: readonly string[] | null): string {
  if (names === null) {
    return '';
  }
  return names.length === 0 ? 'No automations yet' : names.join(', ');
}

function AppRows({
  app,
  automations,
  live,
  nowColumn,
  confirming,
  onConfirm,
  onDelete,
  onMarkChecked,
  onFix,
  onEdit,
  onRemoveFix,
  drafts,
  rows,
}: AppRowsProps) {
  const running = app.windows.length > 0;
  const columns = nowColumn ? 6 : 5;
  if (app.file === null) {
    return (
      <>
        <tr className="group">
          <td colSpan={columns}>
            {app.id}
            <small className="warn">{app.error ?? 'This application could not be read.'}</small>
          </td>
        </tr>
        <ValueDraftRows list={drafts} nowColumn={nowColumn} rows={rows} />
      </>
    );
  }
  const { file } = app;
  if (file.values.length === 0 && drafts.length === 0) {
    return null;
  }
  const build = buildState(file, app.running);
  const checked = checkedText(file, (at) => at.toLocaleDateString(undefined, { dateStyle: 'medium' }));
  const view = live?.kind === 'view' ? live.view : null;
  const size = view?.windowSize ?? null;
  return (
    <>
      <tr className="group">
        <td colSpan={columns}>
          <span className={running ? 'dot running' : 'dot'} />
          {app.file.name}
          <small>
            {app.file.window.executable} · {running ? 'running' : 'not running'}
            {checked === null ? null : ` · ${checked}`}
            {size === null ? null : ` · ${sizeText(size)} now`}
          </small>
          {live?.kind === 'error' ? <small className="warn">{live.message}</small> : null}
          {view?.message === null || view?.message === undefined ? null : (
            <small className="note">{view.message}</small>
          )}
          {build === null ? null : (
            <div className="changed">
              <span className={build.kind === 'changed' ? 'warn' : 'note'}>
                {build.kind === 'changed'
                  ? `Changed since its values were checked (${build.change}). Check that each value still reads right.`
                  : 'Which build its values were checked against is not recorded yet.'}
              </span>
              <span className="with-hint">
                <button type="button" className="btn small" onClick={onMarkChecked}>
                  Mark as checked
                </button>
                <Hint topic="Mark as checked">{MARK_CHECKED_HINT}</Hint>
              </span>
            </div>
          )}
        </td>
      </tr>
      {app.file.values.map((value) => {
        const summary = checkSummary(value.screen.check, file.references);
        const key = `${app.id}/${value.id}`;
        const read = view?.values.find((shown) => shown.id === value.id);
        const cell =
          size === null ? null : nowCell(value.screen, file.sizeChanges, size, readingOf(read), read?.problem ?? null);
        return (
          <tr key={value.id}>
            <td>{value.name}</td>
            <td>
              {summary.colors.map((color) => (
                <span key={hex(color)} className="swatch" style={{ background: hex(color) }} />
              ))}
              {summary.name}
            </td>
            <td className="note">{regionText(value.screen)}</td>
            {nowColumn ? (
              <td>
                {cell === null || size === null ? (
                  <span className="note">{running ? '' : 'Not running'}</span>
                ) : (
                  <NowCellView
                    value={value}
                    cell={cell}
                    size={size}
                    onFix={() => {
                      onFix(value);
                    }}
                    onRemoveFix={() => {
                      onRemoveFix(value.id, size);
                    }}
                  />
                )}
              </td>
            ) : null}
            <td className="note">{usedByText(automations === null ? null : usedBy(automations, app.id, value.id))}</td>
            <td className="end">
              {confirming === key ? (
                <span className="confirm">
                  {drafts.some((each) => each.def.kind === 'fix' && each.def.changes === value.id) ? (
                    <small className="note">Its fix drafts go with it.</small>
                  ) : null}
                  <button
                    type="button"
                    className="btn small"
                    onClick={() => {
                      onConfirm(null);
                    }}
                  >
                    Keep
                  </button>
                  <button
                    type="button"
                    className="btn small danger"
                    onClick={() => {
                      onDelete(value.id);
                    }}
                  >
                    Delete {value.name}
                  </button>
                </span>
              ) : (
                <span className="confirm">
                  <button
                    type="button"
                    className="btn small"
                    onClick={() => {
                      onEdit(value);
                    }}
                  >
                    Edit<span className="visually-hidden"> {value.name}</span>
                  </button>
                  <button
                    type="button"
                    className="btn small"
                    onClick={() => {
                      onConfirm(key);
                    }}
                  >
                    Delete
                  </button>
                </span>
              )}
            </td>
          </tr>
        );
      })}
      <ValueDraftRows list={drafts} nowColumn={nowColumn} rows={rows} />
    </>
  );
}

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly automations: readonly AutomationSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onNew: () => void;
  /** Opens the flow that fixes a saved value at its window's size now, or edits it. */
  readonly onChange: (changing: Changing) => void;
  /** Every saved draft, or null before they are read. */
  readonly drafts: readonly DraftSummary[] | null;
  readonly onFinish: (resumed: Resumed) => void;
  /** Deletes a draft; the page shows why when it fails. */
  readonly onDeleteDraft: (id: string) => Promise<void>;
}

/** The column's heading for what values read now: the window's size when every running window has the same. */
function nowHeading(live: ReadonlyMap<string, LiveApp>): string {
  const sizes = [...live.values()].flatMap((each) =>
    each.kind === 'view' && each.view.windowSize !== null ? [each.view.windowSize] : [],
  );
  const [first] = sizes;
  return first !== undefined && sizes.every((size) => sameSize(size, first))
    ? `At ${sizeText(first)} now`
    : "At the window's size now";
}

export function ValuesPage({
  apps,
  automations,
  loadError,
  onChanged,
  onNew,
  onChange,
  drafts,
  onFinish,
  onDeleteDraft,
}: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const live = useLiveValues(apps);
  const nowColumn = live.size > 0;

  const unfix = async (app: string, value: string, size: SizeDef): Promise<void> => {
    try {
      await removeFix(app, value, size.width, size.height);
      setActionError(null);
      await onChanged();
    } catch (error: unknown) {
      setActionError(errorText(error));
    }
  };

  const remove = async (app: string, value: string): Promise<void> => {
    try {
      await deleteValue(app, value);
      setActionError(null);
      setConfirming(null);
      await onChanged();
    } catch (error: unknown) {
      setActionError(errorText(error));
    }
  };

  const markAsChecked = async (app: string): Promise<void> => {
    try {
      await markChecked(app);
      setActionError(null);
      await onChanged();
    } catch (error: unknown) {
      setActionError(errorText(error));
    }
  };

  const headingRef = useRef<HTMLHeadingElement>(null);
  const rows: DraftRows = {
    apps: apps ?? [],
    onFinish,
    onDelete: async (id) => {
      try {
        await onDeleteDraft(id);
        setActionError(null);
        // The deleted row took the focused button with it.
        headingRef.current?.focus();
      } catch (error: unknown) {
        setActionError(errorText(error));
      }
    },
  };
  const orphans = valueDrafts(drafts, apps ?? [], null);
  const total =
    (apps?.reduce((sum, app) => sum + (app.file?.values.length ?? 0), 0) ?? 0) +
    draftsOf(drafts, 'value').length +
    draftsOf(drafts, 'fix').length;

  return (
    <main className="page">
      <div className="page-head">
        <h1 ref={headingRef} tabIndex={-1}>
          Values
        </h1>
        <span className="lead">What AppyTappy reads from each application&apos;s window.</span>
        <div className="end">
          <button type="button" className="btn primary" onClick={onNew}>
            New value
          </button>
        </div>
      </div>
      {loadError === null ? null : (
        <p className="warn" role="alert">
          {loadError}
        </p>
      )}
      {actionError === null ? null : (
        <p className="warn" role="alert">
          {actionError}
        </p>
      )}
      {apps !== null && total === 0 && apps.every((app) => app.error === null) ? (
        <div className="empty">
          <h2>No values yet</h2>
          <p>
            A value is something AppyTappy reads from an application&apos;s window, such as a number in a certain place
            on screen, whether an icon is showing or how full a bar is. Pick a window, take a screenshot and draw around
            what to read.
          </p>
          <button type="button" className="btn primary" onClick={onNew}>
            New value
          </button>
        </div>
      ) : null}
      {apps !== null && (total > 0 || apps.some((app) => app.error !== null)) ? (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">Value</th>
              <th scope="col">Check</th>
              <th scope="col">Region</th>
              {nowColumn ? (
                <th scope="col">
                  {nowHeading(live)}
                  <Hint topic={nowHeading(live)}>{NOW_HINT}</Hint>
                </th>
              ) : null}
              <th scope="col">Used by</th>
              <th scope="col">
                <span className="note">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {apps.map((app) => (
              <AppRows
                key={app.id}
                app={app}
                automations={automations}
                live={live.get(app.id) ?? null}
                nowColumn={nowColumn}
                confirming={confirming}
                onConfirm={setConfirming}
                onDelete={(value) => {
                  background(remove(app.id, value));
                }}
                onMarkChecked={() => {
                  background(markAsChecked(app.id));
                }}
                onFix={(value) => {
                  onChange({ app: app.id, value, how: 'fix' });
                }}
                onEdit={(value) => {
                  onChange({ app: app.id, value, how: 'edit' });
                }}
                onRemoveFix={(value, size) => {
                  background(unfix(app.id, value, size));
                }}
                drafts={valueDrafts(drafts, apps, app.id)}
                rows={rows}
              />
            ))}
            {orphans.length === 0 ? null : (
              <>
                <tr className="group">
                  <td colSpan={nowColumn ? 6 : 5}>
                    Drafts
                    <small className="warn">Their application is no longer saved.</small>
                  </td>
                </tr>
                <ValueDraftRows list={orphans} nowColumn={nowColumn} rows={rows} />
              </>
            )}
          </tbody>
        </table>
      ) : null}
    </main>
  );
}
