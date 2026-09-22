#!/usr/bin/env bash
# Triage one issue: ask a model for labels, a platform, an assignee and whatever context the
# report is missing, then apply only what survives validation against .github/triage/config.yml.
# The model never talks to GitHub, this script does, so the worst a bad answer can do is pick
# the wrong label out of a fixed list.
#
#   ISSUE=612 DRY_RUN=1 .github/triage/triage.sh
#
# Environment: ISSUE, REPO, GH_TOKEN, TRIAGE_API_KEY, and optionally TRIAGE_BASE_URL,
# TRIAGE_MODEL and DRY_RUN.

set -euo pipefail

tag=triage
# shellcheck source=.github/triage/lib.sh
source "$(git rev-parse --show-toplevel)/.github/triage/lib.sh"

system="$root/.github/triage/prompt.md"
marker="<!-- sonora-triage:needs-info -->"

# The issue as GitHub has it, plus the label names the repository actually carries. A label the
# model picks that nobody ever created is dropped rather than failing the whole edit.
issue_json="$(gh issue view "$issue" --repo "$repo" \
  --json number,title,body,author,labels,comments,state)"

if [ "$(jq -r '.state' <<<"$issue_json")" != "OPEN" ]; then
  say "issue $issue is closed, nothing to do"
  exit 0
fi

existing="$(gh label list --repo "$repo" --limit 200 --json name)"

vocabulary="$(yq -o=json '{"labels": .labels, "platforms": .platforms, "required": .required}' "$config")"
people="$(yq -o=json '.people | with_entries(select(.value.areas | length > 0))' "$config")"

# One user message holding the vocabulary and the report. The body is capped because a report
# with a forty thousand line log pasted into it is still the same triage decision.
user="$(jq -r \
  --argjson vocabulary "$vocabulary" \
  --argjson people "$people" \
  '. as $issue |
   "Labels:\n" + ($vocabulary.labels | tojson) +
   "\n\nPlatforms:\n" + ($vocabulary.platforms | tojson) +
   "\n\nPeople:\n" + ($people | tojson) +
   "\n\nRequired context:\n" + ($vocabulary.required | tojson) +
   "\n\nIssue #" + ($issue.number | tostring) +
   " by " + ($issue.author.login // "unknown") +
   "\nCurrent labels: " + ([$issue.labels[].name] | tojson) +
   "\n\nTitle: " + $issue.title +
   "\n\nBody:\n" + (($issue.body // "") | .[0:12000]) +
   "\n\nComments, oldest first. Context given in one of these counts as given:\n" +
   ([ $issue.comments[]
      | select((.body | contains("sonora-triage")) | not)
      | "- " + (.author.login // "unknown") + ": " + (.body | .[0:4000]) ]
    | .[-10:] | join("\n\n"))' <<<"$issue_json")"

verdict="$(decide "$(compose "$system" "$user")")"

# Everything the model asked for, intersected with what the config allows and what the
# repository actually has. This is the whole safety story.
labels="$(jq -r \
  --argjson vocabulary "$vocabulary" \
  --argjson existing "$existing" \
  '[ (.labels // []) + (.platforms // []) | .[] ]
   | map(select(type == "string"))
   | map(select(. as $l | ($vocabulary.labels | has($l)) or ($vocabulary.platforms | has($l))))
   | map(select(. as $l | $existing | any(.name == $l)))
   | unique | join(",")' <<<"$verdict")"

assignees="$(jq -r \
  --argjson people "$people" \
  '[ (.assignees // [])[] | select(type == "string") | select(. as $p | $people | has($p)) ]
   | unique | .[0:2] | join(",")' <<<"$verdict")"

missing="$(jq -r '[(.missing // [])[] | select(type == "string")] | .[]' <<<"$verdict")"
note="$(jq -r '.note // ""' <<<"$verdict")"

has_label() { jq -e --arg name "$1" 'any(.labels[]; .name == $name)' <<<"$issue_json" >/dev/null; }
asked_already() { jq -e --arg marker "$marker" 'any(.comments[]; .body | contains($marker))' <<<"$issue_json" >/dev/null; }

say "labels: ${labels:-none}"
say "assignees: ${assignees:-none}"
say "missing: $(tr '\n' ';' <<<"$missing")"
say "note: $note"

add=()
remove=()
[ -n "$labels" ] && add+=(--add-label "$labels")
[ -n "$assignees" ] && add+=(--add-assignee "$assignees")
has_label 'needs triage' && remove+=(--remove-label 'needs triage')

# Where the log lives is only worth saying to someone who was asked for one.
where=""
if grep -qi 'log' <<<"$missing"; then
  where="
The log is at \`~/.local/state/sonora/sonora.log\` on Linux, \`~/Library/Caches/sonora/sonora.log\` on macOS and \`%LOCALAPPDATA%\\sonora\\sonora.log\` on Windows. Reproduce the problem, then paste the tail of it. Starting Sonora with \`SONORA_LOG=debug\` makes it more detailed.
"
fi

comment=""
if [ -n "$missing" ] && ! asked_already; then
  comment="$marker
Thanks for the report. Before anyone can look into this, it needs:

$(sed 's/^/- /' <<<"$missing")
$where
Edit the issue or reply with the missing pieces and the label comes off. Left as it is, this closes itself in a couple of weeks.

> [!NOTE]
> Sonora Buddy is an AI. If it read the issue wrong, say so and a maintainer will look.
"
  add+=(--add-label 'needs-info')
elif [ -z "$missing" ] && has_label 'needs-info'; then
  remove+=(--remove-label 'needs-info')
fi

if [ -n "$dry" ]; then
  say "dry run, applying nothing"
  if [ ${#add[@]} -gt 0 ] || [ ${#remove[@]} -gt 0 ]; then
    say "would run: gh issue edit $issue ${add[*]} ${remove[*]}"
  fi
  [ -n "$comment" ] && printf '%s\n' "$comment"
  exit 0
fi

if [ ${#add[@]} -gt 0 ] || [ ${#remove[@]} -gt 0 ]; then
  # An assignee GitHub refuses, or a label deleted between the read and the write, must not
  # take the rest of the edit down with it.
  gh issue edit "$issue" --repo "$repo" "${add[@]}" "${remove[@]}" \
    || say "the edit was rejected, continuing"
fi

if [ -n "$comment" ]; then
  gh issue comment "$issue" --repo "$repo" --body "$comment"
fi
