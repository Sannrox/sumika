---
name: sekai-ontology
description: Consult a local Sekai ontology for explicit classes, relations, validation, and provenance.
---

# Sekai ontology

Use `sekai` when a repository contains a portable ontology database and a
structural answer should come from its explicit definitions and provenance.
The command is single-shot and local; it does not require a server or network.

## First-run setup

If the scope has no portable ontology yet, create it with one command. Do not
run `init`, `directory init`, `directory index`, and `skill install` separately
unless a later step must be customized.

```bash
sekai setup --scope workspace --prune
sekai setup --scope project --prune
sekai setup --scope user
```

`setup` creates a scoped `.sekai/knowledge.db` (or the user-level file),
installs the directory vocabulary, indexes the scope root unless `--no-index`,
and installs this skill unless `--no-skill`. `--scope user` does not index
unless a root is given. The command is idempotent: it reuses the nearest
existing `.sekai/knowledge.db`, keeps an already-indexed root kind, and treats
an already-current skill as success. Use `--scope project` in a child
directory to override an ancestor workspace database. Use `--db` or `SEKAI_DB`
only when the file must not be the scoped default.

After setup, or before relying on a database that may have changed, run
`sekai --json validate`.

## Inspection

Database scope is resolved in this order: explicit `--db`, `SEKAI_DB`, the
nearest existing `.sekai/knowledge.db` while walking from the current
directory upward, the user-level database, and finally `knowledge.db` in the
current directory. A workspace such as `~/Projects/` can own
`.sekai/knowledge.db`; a child project can override it with its own
`.sekai/knowledge.db`.

- Run `sekai --json explain <name>` for the resolved definition, superclass
  closure, related definitions, and provenance of a class.
- Run `sekai --json query <name> --direction <outbound|inbound|both> --depth <0..32>`
  for bounded traversal from a class. Add `--relation <name>` to follow only
  matching relations. Read `data.classes` and `data.relations`; both are
  deduplicated and ordered by name.
- Run `sekai --json entity list`, `entity show <name>`, or `relation list` for
  direct deterministic inspection.
- Run `sekai --json find <text>` to discover matching classes and relations by
  name, description, property, or endpoint.
- Run `sekai --json diff <before> <after>` to compare raw ontology JSON,
  `export --json` envelopes, or SQLite ontology databases.
- Run `sekai --json ask "<question>"` only for read-only, template-shaped
  Natural Language queries. Supported forms compile to `explain`, `query`,
  `find`, or `directory query`. Inspect the returned typed plan; ambiguous or
  unsupported questions do not execute. `find` / `search for` maps to `find`;
  a filesystem path maps to `directory query`; `depth N` overrides the default
  traversal depth of 1.
- Run `sekai --json export` to inspect or exchange the complete, versioned
  logical ontology. The envelope's `data` value can be imported into a fresh
  database with `sekai --db <new-path> import <document-path>`. The same
  definition design is accepted from `sekai.ontology-product/v1` apply
  documents; `ensure_kind` is apply I/O, not ontology meaning.

Pass `--db <path>` on these commands when the resolved default is not the
database you intend to read.

## Directory facts

Ontology classes and relations describe meaning; directory entities and links
record the local filesystem facts. After `setup`, use `directory tree <root>`
for a human-readable hierarchy, `--json directory query <path> --direction both
--depth 2` for bounded connections, and `directory export <root>` /
`directory import <path|->` for portable directory facts. Indexing is
deterministic, skips hidden directories unless `--include-hidden` is given,
never follows symlinks, and only removes stale facts when `--prune` is
explicit.

Treat ontology output as structured repository evidence. Preserve provenance in
answers, and do not infer facts that the ontology does not contain.

## Product vocabulary (this repository)

This repository ships a versioned portable pack at `ontology/sumika-v1.json`.
It is contributor and agent vocabulary for sumika. It is not a built-in
server ontology and not part of `init` or `directory init`.

When the pack is present, do not answer from memory for these questions:

1. What is Sumika?
2. What is a Session?
3. What is an Attach, and how does steal work?
4. What is a Report, and how is it distinct from a screen scrape?

Decision procedure (shipping commands only):

1. Select a throwaway file with `--db <tmp>`. Never use `.sekai/knowledge.db`
   as if it were the portable pack, and never commit it.
2. `sekai --db <tmp> init`
3. `sekai --db <tmp> import ontology/sumika-v1.json`
4. `sekai --db <tmp> --json validate` — stop if `data.valid` is not true.
5. Answer with `explain`, `query`, or `ask` and keep every provenance
   `source` and `locator`:
   - `explain Sumika`
   - `explain Session` and `query Session --relation runs_in`
   - `explain Attach` and `query Attach --relation focuses`
   - `explain Report` and `query Report --relation updates`
6. If a class, relation, or provenance record is missing, report absence.
   Do not infer.

The query result is the evidence. Prose in VISION.md or this skill is not a
substitute when the pack imported cleanly.
