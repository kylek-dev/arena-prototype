#!/usr/bin/env bash
# Creates a match on a development server (`arena-server --dev`) and prints
# one browser join link per seat. Open each link in its own tab or browser.
# Links carry the ticket in the URL fragment, which browsers never send to a
# server; the page removes it from the address bar on load. Development only.
#
# Usage: scripts/dev-match.sh [team size 1-3] [server host:port] [page URL]
set -euo pipefail
team_size="${1:-1}"
server="${2:-127.0.0.1:8080}"
page="${3:-http://localhost:8000/}"
curl -sf -X POST "http://$server/dev/match?team_size=$team_size" |
  python3 -c '
import json, sys, urllib.parse
server, page = sys.argv[1], sys.argv[2]
match = json.load(sys.stdin)
for seat, ticket in enumerate(match["tickets"]):
    team = "A" if seat % 2 == 0 else "B"
    fragment = urllib.parse.urlencode(
        {"server": f"ws://{server}/play", "ticket": ticket, "expires": match["expires"]}
    )
    print(f"seat {seat} (team {team}): {page}#{fragment}")
' "$server" "$page"
