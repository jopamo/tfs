# HACKING

Developer onboarding for `tfs`.

* End-user CLI usage: `README.md`
* Internal architecture and data flow: `DESIGN.md`

This document is **normative** for contributors.
If code behavior conflicts with this document, **the code is wrong**.

---

## Project Overview

`tfs` is a **transactional filesystem operation engine** written in Rust.

It serves two audiences:

1. **Humans**
   A safer alternative to ad-hoc `mv`/`cp` scripting, with dry-runs, explicit manifests, and undo.

2. **AI agents**
   A deterministic, manifest-driven filesystem executor with strict schemas, journals, and reversible operations.
   No guessing. No silent overwrites. No irreversible deletes.

`tfs` is not a shell wrapper. It is a transaction engine.

---

## Core Design Principles

These are invariants, not preferences.

* **Reversibility by construction**
  Every destructive effect must have a journaled undo representation.

* **Explicit scope**
  `tfs` never walks directories or discovers files implicitly.
  All operations must be named in a manifest or CLI arguments.

* **Deterministic execution**
  Given the same manifest and filesystem state, execution order and outcomes must be identical.

* **Manifest-first APIs**
  JSON manifests and schemas are first-class interfaces, not secondary tooling.

* **Safety over convenience**
  Defaults must prevent data loss, even if that makes some workflows more verbose.

If a change violates one of these principles, it is incorrect.

---

## Architecture

High-level architecture, module layout, and execution flow are defined in `DESIGN.md`.

This document focuses on **contributor workflow, invariants, and norms**, not architecture diagrams.

---

## Building and Running

### Prerequisites

* Rust toolchain ≥ **1.86**
* Unix-like filesystem semantics (Linux, macOS; Windows support is future work)

---

### Common Commands

**Build**

```bash
cargo build
```

**Run**

```bash
cargo run -- <args>
```

**Test**

```bash
cargo test
```

**Format**

```bash
cargo fmt --all
```

**Lint**

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

**Local install**

```bash
cargo install --path .
```

---

## Development Usage Examples

**Dump schema (agent tooling)**

```bash
cargo run -- schema
```

**Validate a manifest**

```bash
cargo run -- apply --manifest fs.json --validate-only
```

**Preview a manifest (no writes)**

```bash
cargo run -- apply --manifest fs.json --dry-run --json
```

**Apply a manifest transactionally**

```bash
cargo run -- apply --manifest fs.json --json
```

**Undo from journal**

```bash
cargo run -- undo --journal txn.jsonl
```

---

## Development Conventions

### Code Style

* Always run `cargo fmt`
* `cargo clippy -D warnings` is mandatory
* Prefer explicit state machines over clever control flow
* Avoid `unwrap()` outside tests

---

### Safety and Correctness

These rules are non-negotiable:

* Never delete files
* Never overwrite by default
* Never modify filesystem state during `--dry-run`
* Never infer filesystem state from observation during undo
* Never allow path traversal outside the declared root
* Never silently change destination paths

If something fails, it must fail **loudly and early**.

---

### Filesystem Semantics

When implementing or modifying operations:

* `move`

  * Use `rename()` when same filesystem
  * Fall back to copy + fsync + unlink when cross-device
  * Journal the *actual* behavior used

* `copy`

  * Must record whether destination existed beforehand
  * Must record exact destination path created

* `mkdir`

  * Must be idempotent when `parents=true`
  * Must record whether directory was created by the transaction

Undo logic must rely **only** on journal entries, not filesystem inspection.

---

## Testing

### Unit Tests

* Live close to implementation when possible
* Focus on:

  * path resolution
  * normalization
  * collision handling
  * journal state transitions

### Integration Tests

* Live in `tests/`
* Use temporary directories
* Never rely on global filesystem state
* Always assert undo correctness

### Failure Testing

You **must** test failure paths:

* mid-transaction failure
* cross-device move failure
* permission denial
* collision policy violation

If failure handling is untested, the feature is incomplete.

---

## Journals and Undo

The journal is the **source of truth**.

When working on journaling or undo logic:

* Journal format changes are breaking changes
* Undo must be deterministic and idempotent
* Undo must tolerate partial journals (crash mid-write)
* Never attempt to “guess” undo actions from the filesystem

If undo logic requires filesystem inference, the design is wrong.

---

## Agent-Facing APIs

When modifying any of the following:

* `Plan` schema
* `Operation` schema
* collision policies
* event stream format

You **must**:

* update JSON Schema output
* document changes in `README.md`
* preserve backward compatibility where possible
* version-breaking changes require explicit justification

Agents depend on stability more than humans do.

---

## Contribution Flow

1. Make a focused change
   Small, reviewable diffs only

2. Add or update tests
   Especially for failure and undo paths

3. Run tooling

   ```bash
   cargo fmt
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   ```

4. Submit a PR

   Include:

   * what changed
   * why it changed
   * impact on manifests, journals, or JSON output
   * whether this affects agent workflows

---

## Non-Goals

`tfs` explicitly does **not** aim to:

* walk directories automatically
* guess destination paths
* deduplicate files heuristically
* delete files permanently
* provide “convenience magic” at the cost of safety
* emulate shell behavior

If a feature requires guessing, it does not belong in `tfs`.

---

## Relationship to `txed`

`tfs` and `txed` are siblings.

* `txed` handles **content**
* `tfs` handles **paths and files**

Both share the same philosophy:

* explicit inputs
* deterministic execution
* transactional semantics
* agent-safe APIs

Changes that break this symmetry should be treated with extreme skepticism.

---

## Summary

`tfs` is a **filesystem transaction engine**, not a command wrapper.

As a contributor, your job is to ensure that:

* nothing irreversible happens silently
* nothing ambiguous happens implicitly
* nothing non-deterministic slips through review

If the tool surprises the user—or an agent—it has failed.