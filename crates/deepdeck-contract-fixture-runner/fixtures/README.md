# Data-driven engine contract fixtures

Each JSON fixture contains one canonical Oracle IR rule and the
expected answer from the engine's authoritative `rule_is_executable` contract.
Adding another covered card therefore does not create or recompile a Rust test
harness. The runner depends on `deepdeck-engine-runtime` with default features
disabled, so parser-only edits do not invalidate this binary.

The v1 shape is:

```json
{
  "schemaVersion": "deepdeck-contract-fixture/v1",
  "id": "stable-fixture-id",
  "cardName": "Optional card name",
  "oracleId": "optional-oracle-id",
  "expectedExecutable": true,
  "rule": { "kind": "canonicalRuleKind" }
}
```

After compiling the stable runner once, execute either one fixture or the whole
directory. From the repository root:

```text
deepdeck-contract-fixture-runner crates/deepdeck-contract-fixture-runner/fixtures/stock-up.json
deepdeck-contract-fixture-runner crates/deepdeck-contract-fixture-runner/fixtures
```

Standard output is JSON Lines: one `fixtureResult` event per fixture followed by one
`summary` event. Invalid fixtures and unsupported schema versions produce a
`fixtureError` event and a non-zero exit code. Fixture objects reject unknown
metadata fields so misspelled expectations cannot silently pass.
