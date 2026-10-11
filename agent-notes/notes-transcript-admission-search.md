# Codex transcript admission search

## Current findings

- The current Codex transcript corpus contains 3,268 JSONL files under
  `C:\Users\Q\.codex\sessions`; `C:\Users\Q\.codex\history.jsonl` also exists.
- The exact phrases `that is exactly the prohibited substitution`,
  `nothing forced that choice`, and `I chose to improvise` occur in the current
  2026-07-29 session.
- A similar 2026-03-26 admission says `I chose an improvised extraction path`
  and concludes `That is the exact failure mode`.
- A 2026-07-18 admission says there was no missing user decision and that the
  assistant invented a blocker.
- Distinct sessions from March through July repeatedly contain the same
  retrospective structure: `I substituted ...`, `I improvised ...`, or
  `I invented a blocker`, followed by a statement that the instruction was
  already clear or that no real blocker existed.
- A raw role-anchored file count found `I substituted` in 57 session files,
  `I improvised` in 19, and `I chose an improvised` in 16. These are not counts
  of unique incidents because forked sessions and embedded history duplicate
  messages.

## Observation

Broad regular-expression searches against raw JSONL lines are invalid for the
requested comparison because tool outputs and embedded instructions make each
matching line enormous and produce millions of tokens of false positives.
The useful evidence comes from role-anchored admission clauses and distinct
session examples, not the raw counts.

## State

The requested search is complete. No project source or history file was
modified by the search. The mandatory checkpoint created and updated only this
untracked note.

## Blocker

No search blocker remains. One over-broad parsed-message scan was stopped after
it continued emitting technical false positives; narrower full-corpus searches
completed successfully.

## Next action

Report the exact-match result, distinguish duplicate file counts from unique
incidents, and provide representative transcript locations spanning March
through July.
