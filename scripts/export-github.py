#!/usr/bin/env python3
"""Export repository settings, issues and comments without exposing credentials."""
import json
from pathlib import Path
import subprocess


def gh_json(*args):
    return json.loads(subprocess.check_output(["gh", *args], text=True))


def gh_items(endpoint):
    # Older gh releases lack --slurp; emit one JSON object per line instead.
    output = subprocess.check_output(
        ["gh", "api", "--paginate", endpoint, "--jq", ".[] | @json"], text=True
    )
    return [json.loads(line) for line in output.splitlines() if line]


repo = gh_json("repo", "view", "--json", "nameWithOwner")["nameWithOwner"]
out = Path("planning/exports")
out.mkdir(parents=True, exist_ok=True)
issues = gh_items(f"repos/{repo}/issues?state=all&per_page=100")
records = []
for issue in issues:
    if "pull_request" in issue:
        continue
    issue["exported_comments"] = gh_items(
        f"repos/{repo}/issues/{issue['number']}/comments?per_page=100"
    )
    records.append(issue)
(out / "issues.json").write_text(json.dumps(records, indent=2) + "\n")
settings = gh_json("api", f"repos/{repo}")
public_settings = {key: settings[key] for key in (
    "full_name", "html_url", "description", "visibility", "default_branch",
    "has_issues", "has_wiki", "has_projects", "license",
)}
(out / "repository.json").write_text(json.dumps(public_settings, indent=2) + "\n")
print(f"Exported {len(records)} issues and their comments from {repo}; review before committing.")
