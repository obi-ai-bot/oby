#!/bin/sh
# Runs a command with the Electron app's mock in a browser (npm run web) at brain.config.ts's
# webUrl: the one already running there, else one started for it and stopped after.
set -e
cd "$(dirname "$0")"
url=$(grep -o "webUrl: '[^']*'" brain.config.ts | cut -d"'" -f2)
port=$(echo "$url" | sed -E 's#.*:([0-9]+).*#\1#')
if ! curl -sf -o /dev/null "$url"; then
  (cd ../../apps/electron && [ -d node_modules ] || npm install --silent)
  (cd ../../apps/electron && exec node_modules/.bin/vite --port "$port" --strictPort --logLevel warn) &
  mock=$!
  trap 'kill $mock 2>/dev/null' EXIT INT TERM
  until curl -sf -o /dev/null "$url"; do sleep 0.5; done
  echo "the mock app is at $url"
fi
"$@"
