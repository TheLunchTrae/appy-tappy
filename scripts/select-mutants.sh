#!/bin/sh
# Write the workspace diff to $1 and print the selected mutants, one per line.
# Listing builds nothing and does not touch the mutation report.
set -eu
diff_file=${1:?pass the path to write the diff to}
if [ -n "${MUTANTS_BASE_REF:-}" ]; then
  base_ref="$MUTANTS_BASE_REF"
  if ! base_commit="$(git rev-parse --verify --quiet "$base_ref^{commit}")"; then
    echo "Cannot resolve explicit MUTANTS_BASE_REF '$base_ref'." >&2
    exit 1
  fi
elif base_commit="$(git rev-parse --verify --quiet 'refs/remotes/origin/main^{commit}')"; then
  base_ref="origin/main"
elif base_commit="$(git rev-parse --verify --quiet 'refs/heads/main^{commit}')"; then
  base_ref="main"
else
  echo "Cannot resolve default base refs/remotes/origin/main or refs/heads/main." >&2
  exit 1
fi

if ! merge_base="$(git merge-base HEAD "$base_commit")"; then
  echo "Cannot compute a merge base with $base_ref. Fetch full history with git fetch --unshallow or git fetch origin main." >&2
  exit 1
fi
if ! untracked_files="$(git ls-files --others --exclude-standard)"; then
  echo "Cannot inspect untracked files before mutation diff testing." >&2
  exit 1
fi
if [ -n "$untracked_files" ]; then
  echo "Mutation diff testing cannot include these untracked files:" >&2
  printf '%s\n' "$untracked_files" >&2
  echo "Stage them, or run git add -N <path> for each file so the diff becomes reviewable." >&2
  exit 1
fi

git diff --relative "$merge_base" -- > "$diff_file"

if [ -s "$diff_file" ]; then
  cargo mutants --list --workspace --all-features --cargo-arg=--locked ${MUTANTS_SHARD:+"--shard=$MUTANTS_SHARD"} --in-diff "$diff_file"
fi
