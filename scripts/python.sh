# We source this file in the hooks and the scope shell scripts. Call
# `py ARGS...` to run the project's Python with ARGS. In a run of
# scripts/test.py, that is the interpreter of the run, from ROMINABOX_PYTHON.
# Otherwise it is uv's, with the version in .python-version and the packages
# pinned in uv.lock, as in every command of the README. Set $root to the
# repository's root first.
py() {
  if [ -n "${ROMINABOX_PYTHON:-}" ]; then
    "$ROMINABOX_PYTHON" "$@"
  else
    uv run --locked --project "$root" python "$@"
  fi
}
