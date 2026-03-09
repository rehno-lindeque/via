#!/usr/bin/env bash
# A minimal mock REPL for testing via.
# Reads lines from stdin, echoes them back with a prompt.
# Stays alive until :quit or SIGTERM.
#
# Commands:
#   :quit        — exit
#   :long N      — print N lines of output (default 500)
#   anything     — echo "=> <input>"
PROMPT="${1:-mock>} "
printf '%s' "$PROMPT"
while true; do
  if IFS= read -r line; then
    case "$line" in
      :quit) exit 0 ;;
      :long*)
        n="${line#:long}"
        n="${n// /}"
        n="${n:-500}"
        for i in $(seq 1 "$n"); do
          printf 'line %03d: abcdefghijklmnopqrstuvwxyz 0123456789 the quick brown fox jumps over the lazy dog\n' "$i"
        done
        ;;
      *) echo "=> $line" ;;
    esac
    printf '%s' "$PROMPT"
  else
    sleep 0.1
  fi
done
