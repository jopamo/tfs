# DESIGN

This document describes how `tfs` works internally.

* End-user CLI semantics: `README.md`
* Contributor workflow and norms: `HACKING.md`

This file is **architectural truth**.
If code diverges from it, **the code is wrong**.

---

## High-Level Architecture

At a high level, `tfs`:

1. Parses CLI arguments or a manifest into a unified execution configuration (`Plan`)
2. Resolves explicit inputs (no implicit traversal)
3. Validates and normalizes operations into a canonical form
4. Executes a deterministic filesystem transaction
5. Records a non-lossy journal suitable for undo/resume
6. Commits or rolls back safely
7. Emits structured reports and exit codes

```mermaid
flowchart TD
  CLI[CLI args\nsrc/cli.rs] --> MAIN[src/main.rs]
  MAIN --> MODEL[Plan / Operations\nsrc/model.rs]
  MAIN --> RESOLVE[Path resolution + root confinement\nsrc/resolve.rs]
  MAIN --> VALIDATE[Validation + normalization\nsrc/validate.rs]
  MAIN --> ENGINE[Execution engine\nsrc/engine.rs]
  ENGINE --> OPS[Filesystem operations\nsrc/fsops.rs]
  ENGINE --> JOURNAL[Journal writer\nsrc/journal.rs]
  ENGINE --> TXN[Transaction manager\nsrc/transaction.rs]
  ENGINE --> POLICY[Policy enforcement\nsrc/policy.rs]
  ENGINE --> REPORT[Reporting + events\nsrc/reporter.rs + src/events.rs]
  REPORT --> OUT[stdout / stderr\n(summary | json | agent)]
```

`tfs` is a transaction engine.
The CLI is a thin frontend.

---

## Architectural Invariants

These rules are enforced by design and must remain true:

* No implicit filesystem traversal
* No destructive operation without a reversible representation in the journal
* No silent overwrites by default
* No path escaping outside the declared root
* No non-deterministic destination selection
* JSON output and journals must be complete, deterministic, and non-lossy
* If `transaction = all`, partial application is impossible

---

## Codebase Map

### Language and Edition

* **Language:** Rust
* **Edition:** 2024

---

### Entry Point and CLI

#### `src/main.rs`

Program entry point. Responsibilities:

* parse CLI arguments
* load a manifest or build a `Plan`
* resolve root confinement
* dispatch to engine
* map results to exit codes

No filesystem modifications occur here.

---

#### `src/cli.rs`

Defines CLI flags and subcommands using `clap`.

Suggested commands:

* `tfs schema`
* `tfs apply --manifest FILE [--dry-run] [--validate-only] [--json]`
* `tfs undo --journal FILE`
* `tfs resume --journal FILE` (optional)

CLI flags override manifest values.

Precedence:

```
CLI flags > manifest values > defaults
```

---

## Data Model

### `src/model.rs`

Defines all serializable execution state.

Key types:

* `Plan`
* `Operation`
* `TransactionMode` (`all|op`)
* `CollisionPolicy` (`fail|suffix|hash8|overwrite_with_backup`)
* `SymlinkPolicy` (`follow|skip|error`)
* `Root` and confinement configuration

All input sources compile into the same `Plan`.

---

### Schema Generation

`tfs schema` emits a JSON Schema for `Plan` via `schemars`.

The schema is treated as a stable interface for agents.

---

## Path Resolution and Root Confinement

### `src/resolve.rs`

All paths are interpreted relative to a declared root.

Rules:

* A `Plan.root` must be absolute
* `Operation.src` and `Operation.dst` may be relative
* Every resolved path must satisfy:

  * `canonical(path)` is within `root`
  * no `..` escapes
  * no root rebindings via symlinks unless explicitly allowed

Confinement is enforced before any operation is executed.

---

## Validation and Normalization

### `src/validate.rs`

The validator converts user-provided operations into a canonical, deterministic form.

Responsibilities:

* normalize directory separators
* resolve relative paths
* compute implied parent directories
* insert required `mkdir` operations when `parents = true`
* reject unsupported or ambiguous operations
* enforce collision policy constraints
* enforce “preview implies no writes”

Normalization guarantees:

* same inputs produce identical normalized operation streams
* operations are ordered deterministically

---

## Operation Semantics

### `src/fsops.rs`

Defines the low-level filesystem actions.

Supported operations:

* `mkdir`
* `move`
* `copy`
* `rename` (optional alias for `move` same directory)
* `trash` (optional)

Constraints:

* No `delete` operation exists
* `move` is implemented as:

  * atomic `rename()` when source and destination are on the same filesystem
  * copy + fsync + unlink when cross-device, with journal recording the actual behavior

All operations report:

* bytes moved/copied
* final resolved destination path
* whether overwrite occurred
* any metadata changes (mode/mtime/owner if relevant)

---

## Transaction Model

### `src/transaction.rs`

`tfs` supports two modes:

* `transaction = all`
  Commit only if every operation succeeds

* `transaction = op`
  Each operation commits independently but is still journaled for undo

Key invariant:

* In `all` mode, partial application is impossible unless there is a bug

Implementation approach:

* stage operations when possible
* apply operations in order
* on failure:

  * compute undo steps from journal entries already marked `ok`
  * execute undo steps in reverse order
  * mark journal as `aborted`

---

## Journal

### `src/journal.rs`

The journal is the authority for undo and resume.

Format:

* NDJSON (JSON Lines)
* append-only
* fsync after each record when not in dry-run mode

Each record includes:

* `id` stable operation id
* `ts` monotonic ordering
* `op`
* resolved `src` and `dst`
* collision resolution details (final chosen destination)
* status transition: `start|ok|fail|undone`
* undo metadata:

  * for move: original location
  * for copy: created destination path
  * for overwrite_with_backup: backup location
* optional hashes or stat snapshots when configured

Undo never infers state from the filesystem.
Undo uses the journal.

---

## Policy Enforcement

### `src/policy.rs`

Policies apply uniformly across CLI and manifest execution.

Policies include:

* collision policy (default fail)
* symlink policy
* file type restrictions (optional)
* max bytes moved (optional)
* forbid cross-device moves (optional)
* require explicit `--allow-overwrite` to enable overwrite policies
* require `--dry-run` before apply (optional “two-phase” gate)

Policy failures are explicit exit code `2`.

---

## Execution Engine

### `src/engine.rs`

The engine runs the normalized operation stream.

Lifecycle:

1. validate + normalize plan
2. run preflight checks
3. if `validate-only`, stop
4. if `dry-run`, simulate and emit a preview report (no writes, no journal mutations)
5. otherwise:

   * write journal `start`
   * execute op
   * write journal `ok` or `fail`
   * on failure in `transaction=all`, undo everything already applied
6. emit report + exit code

The engine contains no CLI parsing.

---

## Reporting and Events

### `src/events.rs`

Defines structured events emitted to stdout in JSON mode.

Events are deterministic and non-lossy:

* `plan_validated`
* `op_planned`
* `op_started`
* `op_completed`
* `op_failed`
* `txn_committed`
* `txn_aborted`
* `undo_started`
* `undo_completed`

### `src/reporter.rs`

Aggregates results into:

* human summary
* machine JSON
* agent format

Output selection never changes the behavior of the engine.

---

## Exit Codes

### `src/exit_codes.rs`

Canonical exit codes:

* `0` success
* `1` operational failure (I/O, permissions, etc.)
* `2` policy failure
* `3` transactional failure (aborted, partial prevented)

---

## Data Flow

### Apply Manifest Mode (`tfs apply --manifest plan.json`)

1. parse CLI + manifest
2. construct `Plan`
3. resolve paths under root
4. validate + normalize into canonical op stream
5. dry-run preview or apply
6. journal records transitions
7. commit or undo
8. emit report + exit code

---

### Undo Mode (`tfs undo --journal txn.jsonl`)

1. parse journal
2. validate journal integrity
3. compute undo operations deterministically
4. execute undo operations in reverse order
5. append undo records
6. report status

Undo does not require original manifest.

---

## Determinism Guarantees

`tfs` guarantees:

* stable operation ordering
* stable collision resolution when policy is deterministic
* stable JSON output
* journal is sufficient to undo/resume without guessing

---

## Decision Log

* **Journal as source of truth**
  Enables undo/resume without filesystem inference

* **No delete**
  Safety by construction, supports agent workflows

* **Root confinement**
  Prevents accidental path escapes and unsafe plans

* **Collision policy explicit**
  Avoids silent overwrites and nondeterminism

* **NDJSON events**
  Streamable, tool-friendly, agent-compatible

---

## Non-Goals

`tfs` explicitly does **not**:

* walk directories
* choose files automatically
* infer destination naming
* perform heuristic dedupe
* delete files permanently
* “fix” unsafe manifests

---

## Summary

`tfs` is a deterministic, manifest-driven filesystem transaction engine.

It is designed to be:

* safe by default
* reversible by construction
* explicit in all inputs
* suitable for autonomous agents

This makes it a natural filesystem companion to `txed`.
