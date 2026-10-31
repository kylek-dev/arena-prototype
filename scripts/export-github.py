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

project_path = Path("planning/github-project.json")
if project_path.exists():
    project = json.loads(project_path.read_text())
    selector = [str(project["number"]), "--owner", project["owner"], "--format", "json"]
    metadata = gh_json("project", "view", *selector)
    fields = gh_json("project", "field-list", *selector, "--limit", "1000")
    items = gh_json("project", "item-list", *selector, "--limit", "1000")
    # Fail rather than silently writing a partial export as the project grows.
    for data, key in ((fields, "fields"), (items, "items")):
        if len(data[key]) != data["totalCount"]:
            raise RuntimeError("Project exceeds export limit; add pagination before exporting")
    query = """query($owner:String!, $number:Int!) {
      user(login:$owner) { projectV2(number:$number) {
        views(first:100) { totalCount nodes { id name number layout filter
          groupByFields(first:100) { nodes { ... on ProjectV2FieldCommon { id name } } }
          verticalGroupByFields(first:100) { nodes { ... on ProjectV2FieldCommon { id name } } }
        } }
        repositories(first:100) { totalCount nodes { nameWithOwner } }
      } }
    }"""
    result = gh_json("api", "graphql", "-f", f"query={query}",
                     "-f", f"owner={project['owner']}", "-F", f"number={project['number']}")
    presentation = result["data"]["user"]["projectV2"]
    for key in ("views", "repositories"):
        if len(presentation[key]["nodes"]) != presentation[key]["totalCount"]:
            raise RuntimeError("Project presentation exceeds export limit")
    snapshot = {"project": metadata, "fields": fields, "items": items, **presentation}
    (out / "project.json").write_text(json.dumps(snapshot, indent=2) + "\n")
    print(f"Exported public project {project['number']}: {len(items['items'])} items.")
