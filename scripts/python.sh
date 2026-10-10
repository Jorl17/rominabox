# We source this file in the hooks and the scope shell scripts. Call
# `py ARGS...` to run the project's Python with ARGS. In a run of
# scripts/test.py, that is the interpreter of the run, from ROMINABOX_PYTHON.
# Otherwise it is uv's, with the version in .python-version and exactly the
# packages pinned in uv.lock, as in a fresh clone (without any package
# installed by hand). Set $root to the repository's root first.
py() {
  # We run Python in UTF-8 mode, as in scripts/test.py.
  export PYTHONUTF8=1
  if [ -n "${ROMINABOX_PYTHON:-}" ]; then
    "$ROMINABOX_PYTHON" "$@"
  else
    uv run --locked --exact --project "$root" python "$@"
  fi
}
