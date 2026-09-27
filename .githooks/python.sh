# We source this file in the hooks to set $python, the Python they run. It
# is ROMINABOX_PYTHON when set, otherwise the usual name on the platform. On
# Windows (Git Bash) `python3` is the Microsoft Store stub and the interpreter
# is `python`. On macOS, Linux and other POSIX systems it is `python3`.
python=${ROMINABOX_PYTHON:-}
if [ -z "$python" ]; then
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) python=python ;;
    *) python=python3 ;;
  esac
fi
