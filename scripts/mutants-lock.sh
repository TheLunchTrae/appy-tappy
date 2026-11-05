#!/bin/sh

# Source this file, acquire the lock, and install an EXIT trap that calls
# mutants_task_release_lock before starting cargo-mutants.

acquire_mutants_task_lock() {
  mutants_task_mode="${1:-unknown}"
  mutants_task_lock_dir="${MUTANTS_TASK_LOCK_DIR:-target/standards-mutants.lock}"
  mutants_task_lock_parent="$(dirname "$mutants_task_lock_dir")"
  mkdir -p "$mutants_task_lock_parent"

  if ! mkdir "$mutants_task_lock_dir" 2> /dev/null; then
    echo "Another Rust mutation task is already active; lock: $mutants_task_lock_dir" >&2
    if [ -r "$mutants_task_lock_dir/owner" ]; then
      echo "Lock owner:" >&2
      sed 's/^/  /' "$mutants_task_lock_dir/owner" >&2
    fi
    echo "If no mutation task or process-group descendant is active, remove the stale lock directory and retry." >&2
    return 1
  fi

  mutants_task_lock_owned=true
  mutants_task_lock_release_allowed=true
  if ! printf 'pid=%s\nmode=%s\nstarted_utc=%s\n' \
    "$$" "$mutants_task_mode" "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
    > "$mutants_task_lock_dir/owner"; then
    rm -f "$mutants_task_lock_dir/owner"
    rmdir "$mutants_task_lock_dir" 2> /dev/null || true
    mutants_task_lock_owned=false
    echo "Could not record ownership of Rust mutation task lock: $mutants_task_lock_dir" >&2
    return 1
  fi
}

mutants_task_forward_signal() {
  if [ "$mutants_task_child_signal_status" -eq 0 ]; then
    mutants_task_child_signal="$1"
    mutants_task_child_signal_status="$2"
  fi
  if [ -n "${mutants_task_child_group:-}" ]; then
    mutants_task_signal_group "$1" "$mutants_task_child_group"
    mutants_task_start_signal_watchdog
  fi
}

mutants_task_signal_group() {
  kill "-$1" "-$2" 2> /dev/null || true
}

mutants_task_group_exists() {
  kill -0 "-$1" 2> /dev/null
}

mutants_task_wait_for_group_exit() {
  mutants_task_group_wait=0
  while mutants_task_group_exists "$1"; do
    if [ "$mutants_task_group_wait" -ge "$2" ]; then
      return 1
    fi
    sleep 0.05
    mutants_task_group_wait=$((mutants_task_group_wait + 1))
  done
}

mutants_task_start_signal_watchdog() {
  if [ -n "${mutants_task_child_watchdog_pid:-}" ]; then
    return
  fi
  mutants_task_watchdog_group="$mutants_task_child_group"
  setsid sh -s -- "$mutants_task_watchdog_group" << 'EOF' &
sleep 5
if kill -0 "-$1" 2> /dev/null; then
  kill -TERM "-$1" 2> /dev/null || true
fi
sleep 5
if kill -0 "-$1" 2> /dev/null; then
  kill -KILL "-$1" 2> /dev/null || true
fi
EOF
  mutants_task_child_watchdog_pid=$!
}

mutants_task_cancel_signal_watchdog() {
  if [ -z "${mutants_task_child_watchdog_pid:-}" ]; then
    return
  fi
  kill -KILL "-$mutants_task_child_watchdog_pid" 2> /dev/null || true
  wait "$mutants_task_child_watchdog_pid" 2> /dev/null || true
  mutants_task_child_watchdog_pid=""
}

mutants_task_stop_process_group() {
  mutants_task_stopping_group="$1"
  if ! mutants_task_group_exists "$mutants_task_stopping_group"; then
    return 0
  fi

  if [ "$mutants_task_child_signal_status" -eq 0 ] &&
    mutants_task_wait_for_group_exit "$mutants_task_stopping_group" 100; then
    return 0
  fi
  mutants_task_signal_group TERM "$mutants_task_stopping_group"
  if mutants_task_wait_for_group_exit "$mutants_task_stopping_group" 100; then
    return 0
  fi
  mutants_task_signal_group KILL "$mutants_task_stopping_group"
  if mutants_task_wait_for_group_exit "$mutants_task_stopping_group" 100; then
    return 0
  fi

  echo "Rust mutation process group $mutants_task_stopping_group survived KILL; retaining $mutants_task_lock_dir because descendant shutdown could not be confirmed." >&2
  return 1
}

run_mutants_task_child() {
  if [ "$#" -eq 0 ]; then
    echo "run_mutants_task_child requires a command." >&2
    return 2
  fi
  if ! command -v setsid > /dev/null 2>&1; then
    echo "run_mutants_task_child requires setsid to create an isolated mutation process group." >&2
    return 2
  fi

  mutants_task_child_pid=""
  mutants_task_child_group=""
  mutants_task_child_watchdog_pid=""
  mutants_task_child_signal=""
  mutants_task_child_signal_status=0
  trap 'mutants_task_forward_signal HUP 129' HUP
  trap 'mutants_task_forward_signal INT 130' INT
  trap 'mutants_task_forward_signal TERM 143' TERM
  mutants_task_lock_release_allowed=false
  setsid "$@" <&0 &
  mutants_task_child_pid=$!
  mutants_task_child_group="$mutants_task_child_pid"
  if [ -n "$mutants_task_child_signal" ]; then
    mutants_task_signal_group "$mutants_task_child_signal" "$mutants_task_child_group"
    mutants_task_start_signal_watchdog
  fi

  mutants_task_child_status=0
  while :; do
    if wait "$mutants_task_child_pid"; then
      mutants_task_child_status=0
    else
      mutants_task_child_status=$?
    fi
    if ! kill -0 "$mutants_task_child_pid" 2> /dev/null; then
      break
    fi
  done
  mutants_task_child_pid=""
  mutants_task_cancel_signal_watchdog

  if ! mutants_task_stop_process_group "$mutants_task_child_group"; then
    mutants_task_child_group=""
    mutants_task_cancel_signal_watchdog
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    return 1
  fi
  mutants_task_child_group=""
  mutants_task_cancel_signal_watchdog
  mutants_task_lock_release_allowed=true

  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM
  if [ "$mutants_task_child_signal_status" -ne 0 ]; then
    return "$mutants_task_child_signal_status"
  fi
  return "$mutants_task_child_status"
}

mutants_task_release_lock() {
  if [ "${mutants_task_lock_owned:-false}" != "true" ]; then
    return
  fi
  if [ "${mutants_task_lock_release_allowed:-false}" != "true" ]; then
    echo "Refusing to remove Rust mutation task lock $mutants_task_lock_dir because process-group shutdown was not confirmed." >&2
    return 1
  fi

  rm -f "$mutants_task_lock_dir/owner"
  if ! rmdir "$mutants_task_lock_dir"; then
    echo "Could not remove Rust mutation task lock: $mutants_task_lock_dir" >&2
    return 1
  fi
  mutants_task_lock_owned=false
}

run_mutants_lock_self_tests() {
  mutants_lock_test_root="$(mktemp -d)"
  mutants_lock_test_pid=""
  mutants_lock_descendant_pid=""
  mutants_lock_test_cleanup() {
    if [ -n "$mutants_lock_test_pid" ]; then
      kill "$mutants_lock_test_pid" 2> /dev/null || true
      wait "$mutants_lock_test_pid" 2> /dev/null || true
    fi
    if [ -n "$mutants_lock_descendant_pid" ]; then
      kill -KILL "$mutants_lock_descendant_pid" 2> /dev/null || true
    fi
    rm -rf "$mutants_lock_test_root"
  }
  trap mutants_lock_test_cleanup EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM

  case "$0" in
    /*) mutants_lock_script="$0" ;;
    *) mutants_lock_script="$(pwd)/$0" ;;
  esac
  mutants_lock_test_dir="$mutants_lock_test_root/mutation.lock"
  mutants_lock_started="$mutants_lock_test_root/started"
  mutants_lock_release="$mutants_lock_test_root/release"

  MUTANTS_TASK_LOCK_DIR="$mutants_lock_test_dir" \
    sh -c '
      . "$1"
      acquire_mutants_task_lock self-test
      trap mutants_task_release_lock EXIT
      : >"$2"
      while [ ! -f "$3" ]; do sleep 0.05; done
    ' sh "$mutants_lock_script" "$mutants_lock_started" "$mutants_lock_release" &
  mutants_lock_test_pid=$!

  mutants_lock_wait=0
  while [ ! -f "$mutants_lock_started" ] && [ "$mutants_lock_wait" -lt 100 ]; do
    sleep 0.05
    mutants_lock_wait=$((mutants_lock_wait + 1))
  done
  if [ ! -f "$mutants_lock_started" ]; then
    echo "Mutation lock self-test timed out waiting for its holder." >&2
    exit 1
  fi
  if ! grep -qx 'mode=self-test' "$mutants_lock_test_dir/owner"; then
    echo "Mutation lock owner metadata did not identify its mode." >&2
    exit 1
  fi

  if MUTANTS_TASK_LOCK_DIR="$mutants_lock_test_dir" \
    sh -c '. "$1"; acquire_mutants_task_lock self-test' sh "$mutants_lock_script" \
    > "$mutants_lock_test_root/contender.log" 2>&1; then
    echo "Mutation lock self-test allowed a concurrent holder." >&2
    exit 1
  fi
  if ! grep -q 'remove the stale lock directory' "$mutants_lock_test_root/contender.log"; then
    echo "Mutation lock contention did not provide stale-lock recovery guidance." >&2
    exit 1
  fi

  : > "$mutants_lock_release"
  if ! wait "$mutants_lock_test_pid"; then
    echo "Mutation lock holder failed during the self-test." >&2
    exit 1
  fi
  mutants_lock_test_pid=""
  if [ -e "$mutants_lock_test_dir" ]; then
    echo "Mutation lock survived a clean task exit." >&2
    exit 1
  fi

  MUTANTS_TASK_LOCK_DIR="$mutants_lock_test_dir" \
    sh -c '. "$1"; acquire_mutants_task_lock self-test; trap mutants_task_release_lock EXIT' \
    sh "$mutants_lock_script"
  if [ -e "$mutants_lock_test_dir" ]; then
    echo "Mutation lock survived a second soft exit." >&2
    exit 1
  fi

  mutants_lock_unconfirmed_dir="$mutants_lock_test_root/unconfirmed.lock"
  mutants_lock_unconfirmed_log="$mutants_lock_test_root/unconfirmed.log"
  MUTANTS_TASK_LOCK_DIR="$mutants_lock_unconfirmed_dir" \
    sh -c '
      . "$1"
      acquire_mutants_task_lock unconfirmed-test
      mutants_task_lock_release_allowed=false
      if mutants_task_release_lock > "$2" 2>&1; then
        echo "Mutation task released an unconfirmed process-group lock." >&2
        exit 1
      fi
      if [ ! -d "$MUTANTS_TASK_LOCK_DIR" ]; then
        echo "Mutation task removed an unconfirmed process-group lock." >&2
        exit 1
      fi
      rm -f "$MUTANTS_TASK_LOCK_DIR/owner"
      rmdir "$MUTANTS_TASK_LOCK_DIR"
    ' sh "$mutants_lock_script" "$mutants_lock_unconfirmed_log"
  if ! grep -q 'process-group shutdown was not confirmed' "$mutants_lock_unconfirmed_log"; then
    echo "Unconfirmed mutation-lock retention lacked process-group guidance." >&2
    exit 1
  fi

  mutants_lock_signal_dir="$mutants_lock_test_root/signal.lock"
  mutants_lock_signal_started="$mutants_lock_test_root/signal-started"
  mutants_lock_signal_settled="$mutants_lock_test_root/signal-settled"
  mutants_lock_signal_released_early="$mutants_lock_test_root/signal-released-early"
  mutants_lock_signal_child="$mutants_lock_test_root/signal-child.sh"
  cat > "$mutants_lock_signal_child" << 'EOF'
#!/bin/sh
on_term() {
  sleep 0.2
  if [ ! -d "$MUTANTS_SIGNAL_LOCK" ]; then
    : > "$MUTANTS_SIGNAL_RELEASED_EARLY"
  fi
  : > "$MUTANTS_SIGNAL_SETTLED"
  exit 0
}
trap on_term TERM
: > "$MUTANTS_SIGNAL_STARTED"
while :; do sleep 0.05; done
EOF

  MUTANTS_TASK_LOCK_DIR="$mutants_lock_signal_dir" \
    MUTANTS_SIGNAL_LOCK="$mutants_lock_signal_dir" \
    MUTANTS_SIGNAL_STARTED="$mutants_lock_signal_started" \
    MUTANTS_SIGNAL_SETTLED="$mutants_lock_signal_settled" \
    MUTANTS_SIGNAL_RELEASED_EARLY="$mutants_lock_signal_released_early" \
    sh -c '
      . "$1"
      acquire_mutants_task_lock signal-test
      trap mutants_task_release_lock EXIT
      run_mutants_task_child sh "$2"
    ' sh "$mutants_lock_script" "$mutants_lock_signal_child" &
  mutants_lock_test_pid=$!

  mutants_lock_wait=0
  while [ ! -f "$mutants_lock_signal_started" ] && [ "$mutants_lock_wait" -lt 100 ]; do
    sleep 0.05
    mutants_lock_wait=$((mutants_lock_wait + 1))
  done
  if [ ! -f "$mutants_lock_signal_started" ]; then
    echo "Mutation lock signal self-test timed out waiting for its child." >&2
    exit 1
  fi

  kill -TERM "$mutants_lock_test_pid"
  if [ ! -d "$mutants_lock_signal_dir" ]; then
    echo "Mutation task released its lock before the signaled child settled." >&2
    exit 1
  fi
  mutants_lock_signal_status=0
  if wait "$mutants_lock_test_pid"; then
    echo "Mutation task ignored a targeted parent TERM signal." >&2
    exit 1
  else
    mutants_lock_signal_status=$?
  fi
  mutants_lock_test_pid=""
  if [ "$mutants_lock_signal_status" -ne 143 ]; then
    echo "Mutation task returned $mutants_lock_signal_status instead of 143 after TERM." >&2
    exit 1
  fi
  if [ ! -f "$mutants_lock_signal_settled" ]; then
    echo "Mutation task did not wait for its signaled child to settle." >&2
    exit 1
  fi
  if [ -e "$mutants_lock_signal_released_early" ] || [ -e "$mutants_lock_signal_dir" ]; then
    echo "Mutation task lock ownership did not cover child signal settlement." >&2
    exit 1
  fi

  mutants_lock_descendant_dir="$mutants_lock_test_root/descendant.lock"
  mutants_lock_descendant_ready="$mutants_lock_test_root/descendant-ready"
  mutants_lock_descendant_pid_file="$mutants_lock_test_root/descendant.pid"
  mutants_lock_descendant_released_early="$mutants_lock_test_root/descendant-released-early"
  mutants_lock_descendant_child="$mutants_lock_test_root/descendant-child.sh"
  cat > "$mutants_lock_descendant_child" << 'EOF'
#!/bin/sh
sh -c '
  trap "" TERM
  printf "%s\n" "$$" > "$MUTANTS_DESCENDANT_PID_FILE"
  : > "$MUTANTS_DESCENDANT_READY"
  while [ -d "$MUTANTS_DESCENDANT_LOCK" ]; do sleep 0.05; done
  : > "$MUTANTS_DESCENDANT_RELEASED_EARLY"
  while :; do sleep 1; done
' &
wait "$!"
EOF

  MUTANTS_TASK_LOCK_DIR="$mutants_lock_descendant_dir" \
    MUTANTS_DESCENDANT_LOCK="$mutants_lock_descendant_dir" \
    MUTANTS_DESCENDANT_READY="$mutants_lock_descendant_ready" \
    MUTANTS_DESCENDANT_PID_FILE="$mutants_lock_descendant_pid_file" \
    MUTANTS_DESCENDANT_RELEASED_EARLY="$mutants_lock_descendant_released_early" \
    sh -c '
      . "$1"
      acquire_mutants_task_lock descendant-test
      trap mutants_task_release_lock EXIT
      run_mutants_task_child sh "$2"
    ' sh "$mutants_lock_script" "$mutants_lock_descendant_child" &
  mutants_lock_test_pid=$!

  mutants_lock_wait=0
  while [ ! -f "$mutants_lock_descendant_ready" ] && [ "$mutants_lock_wait" -lt 100 ]; do
    sleep 0.05
    mutants_lock_wait=$((mutants_lock_wait + 1))
  done
  if [ ! -f "$mutants_lock_descendant_ready" ]; then
    echo "Mutation lock descendant self-test timed out waiting for its grandchild." >&2
    exit 1
  fi
  IFS= read -r mutants_lock_descendant_pid < "$mutants_lock_descendant_pid_file"

  kill -TERM "$mutants_lock_test_pid"
  mutants_lock_descendant_status=0
  if wait "$mutants_lock_test_pid"; then
    echo "Mutation task ignored a targeted parent TERM with a live grandchild." >&2
    exit 1
  else
    mutants_lock_descendant_status=$?
  fi
  mutants_lock_test_pid=""
  if [ "$mutants_lock_descendant_status" -ne 143 ]; then
    echo "Mutation task returned $mutants_lock_descendant_status instead of 143 after descendant TERM." >&2
    exit 1
  fi

  mutants_lock_wait=0
  while [ ! -e "$mutants_lock_descendant_released_early" ] &&
    kill -0 "$mutants_lock_descendant_pid" 2> /dev/null &&
    [ "$mutants_lock_wait" -lt 20 ]; do
    sleep 0.05
    mutants_lock_wait=$((mutants_lock_wait + 1))
  done
  if [ -e "$mutants_lock_descendant_released_early" ]; then
    echo "Mutation task released its lock while a TERM-resistant grandchild survived." >&2
    exit 1
  fi
  if kill -0 "$mutants_lock_descendant_pid" 2> /dev/null; then
    echo "Mutation task did not stop its TERM-resistant grandchild." >&2
    exit 1
  fi
  mutants_lock_descendant_pid=""
  if [ -e "$mutants_lock_descendant_dir" ]; then
    echo "Mutation task lock survived confirmed descendant shutdown." >&2
    exit 1
  fi

  trap - EXIT HUP INT TERM
  mutants_lock_test_cleanup
  echo "Mutation task lock self-tests passed."
}

if [ "${1:-}" = "--self-test" ]; then
  set -eu
  run_mutants_lock_self_tests
fi
