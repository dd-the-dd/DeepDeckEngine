# Native parser test workflow

Card migration uses a data-driven parser loop by default. The historical Rust
integration suites remain regression assets, but they are not the edit-loop
entry point because Cargo links every file under `crates/*/tests` as a separate
test harness.

## Fast native profile

Inspect the boundary without compiling or running anything:

```powershell
pwsh ./scripts/test-native.ps1 -InventoryOnly `
  -ReportJson .tmp/native-test-inventory.json
```

After parser sources change, reserve the shared Cargo lane and rebuild the one
stable runner:

```powershell
pwsh ./scripts/test-native.ps1 -Build `
  -TargetDirectory .tmp/targets/card-migration `
  -ReportJson .tmp/native-test-run.json
```

When only JSON fixtures change, omit `-Build`. The profile then invokes no
Cargo command: it runs the architecture and unit-test-layout guards, followed
by every fixture in `crates/deepdeck-parser-fixture-runner/fixtures` through the
existing runner binary.

Runner reuse is valid only when `run-parser-fixture.ps1` verifies its metadata.
The metadata binds the executable hash to the current parser sources,
compile-time vocabulary/catalog data, relevant manifests, lockfile, target
directory and debug profile. Missing or stale metadata fails before fixtures
run. A no-build execution without this verification is diagnostic only, never
a regression certificate.

The report separates the discoverable historical harness count from the native
profile's compiled target count. The native profile compiles no Rust test
harness; it uses one ordinary runner binary and a set of JSON cases.

## Focused card proof

Use `scripts/run-parser-fixture.ps1` for one parser fixture and
`scripts/test-card-migration.ps1` for an exact Rust target when a private edge
test is required. A parser-only green result is insufficient after any semantic
or runtime change: the exact parser/runtime contract and runtime behavior
fixtures remain mandatory. Those proofs continue to use their stable
data-driven runners through the strict audit workflow.

## Full regression remains opt-in

The native profile does not delete or disable historical tests. Run the
relevant explicit target during development, and retain the complete suites at
checkpoints and in CI. In particular, do not replace these unique edge proofs
with card snapshots:

- target multiplicity, selector composition, and transactional rollback in
  `src/oracle/canonical/automata/tests.rs`;
- source binding and anaphora in
  `src/oracle/canonical/automata/sequence_context_tests.rs`;
- arithmetic precedence and explicit rounding in
  `src/oracle/canonical/automata/arithmetic_expression_tests.rs` and
  `src/oracle/canonical/effects/numeric_tests.rs`;
- atomic cost payment in `src/engine/cost_payment_tests.rs`;
- live/snapshot/LKI behavior in `src/engine/tests/source_lki_runtime.rs` and
  `src/engine/tests/snapshot_bindings.rs`.

No historical test file should be removed merely because a similarly named
JSON fixture exists. Deletion requires semantic assertion equivalence,
including sibling, near-miss, diagnostic-state, binding, and runtime checks.
