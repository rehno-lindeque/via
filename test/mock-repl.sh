#!/usr/bin/env bash
# A minimal mock REPL for testing via.
# Reads lines from stdin, echoes them back with a prompt.
# Stays alive until :quit or SIGTERM.
PROMPT="${1:-mock>} "
printf '%s' "$PROMPT"
while true; do
  if IFS= read -r line; then
    case "$line" in
      :quit) exit 0 ;;
      *) echo "=> $line" ;;
    esac
    printf '%s' "$PROMPT"
  else
    # EOF on stdin — sleep and retry (teetty reopens the PTY)
    sleep 0.1
  fi
done
