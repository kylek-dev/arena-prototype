#!/usr/bin/env python3
"""Export repository settings, issues and comments without exposing credentials."""
import json
from pathlib import Path
import subprocess


def gh_json(*args):
    return json.loads(subprocess.check_output(["gh", *args], text=True))


repo = gh_json("repo", "view", "--json", "nameWithOwner")["nameWithOwner"]
out = Path("planning/exports")
out.mkdir(parents=True, exist_ok=True)
issues = gh_json("api", "--paginate", "--slurp", f"repos/{repo}/issues?state=all&per_page=100")
records = []
for page in issues:
    for issue in page:
        if "pull_request" in issue:
            continue
        issue["exported_comments"] = [
            comment
            for comments in gh_json(
                "api", "--paginate", "--slurp",
                f"repos/{repo}/issues/{issue['number']}/comments?per_page=100",
            )
            for comment in comments
        ]
        records.append(issue)
(out / "issues.json").write_text(json.dumps(records, indent=2) + "\n")
settings = gh_json("api", f"repos/{repo}")
public_settings = {key: settings[key] for key in (
    "full_name", "html_url", "description", "visibility", "default_branch",
    "has_issues", "has_wiki", "has_projects", "license",
)}
(out / "repository.json").write_text(json.dumps(public_settings, indent=2) + "\n")
print(f"Exported {len(records)} issues and their comments from {repo}; review before committing.")
