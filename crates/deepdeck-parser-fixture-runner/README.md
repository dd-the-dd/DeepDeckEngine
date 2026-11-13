# Data-driven Oracle parser fixtures

This stable runner parses complete `OracleCardParseRequest` values from JSON.
Adding a card, a generalized sibling, or a negative near-miss therefore does
not create a Rust integration-test target and does not relink another test
binary. Rebuild the runner once after parser source changes, then execute any
number of fixtures directly.

The runner validates three separate parser contracts:

- card and ability status, recognizer provenance, and ability kind;
- exact canonical rule JSON or selected JSON-pointer assertions;
- structured failure diagnostics, including automaton, state, expected/found
  tokens, token index, and transition-trace states.

Unknown fixture, case, request, expectation, and assertion fields are rejected.
This prevents misspelled expectations from silently passing.

## Schema v1

One fixture groups related cases so the exact card, sibling patterns, and
near-misses can execute in one process:

```json
{
  "schemaVersion": "deepdeck-parser-fixture/v1",
  "id": "stable/family-id",
  "cardName": "Optional display name",
  "oracleId": "optional-oracle-id",
  "cases": [
    {
      "id": "exact-card",
      "request": {
        "cardName": "Card Name",
        "typeLine": "Instant",
        "manaCost": "{U}",
        "oracleText": "Draw a card.",
        "layout": null,
        "faces": []
      },
      "expect": {
        "outcome": "success",
        "cardStatus": "canonical",
        "abilityCount": 1,
        "abilities": [
          {
            "index": 0,
            "status": "canonical",
            "recognizer": "native",
            "abilityType": "spellAbility",
            "ruleAssertions": [
              { "pointer": "/effects/0/kind", "equals": "drawCards" }
            ]
          }
        ]
      }
    }
  ]
}
```

A success ability may use `rule` for an exact canonical-IR comparison,
`ruleAssertions` for deliberately narrow JSON-pointer checks, or both.
Successful cases reject unexpected diagnostics unless `allowDiagnostics` is
explicitly true.

A failure expectation uses the same card/ability shape plus one or more
diagnostic matchers:

```json
{
  "outcome": "failure",
  "cardStatus": "unsupported",
  "abilities": [
    { "index": 0, "status": "unsupported", "recognizer": "native" }
  ],
  "diagnostics": [
    {
      "abilityIndex": 0,
      "code": "oracle_machine_failure",
      "automaton": "effect-sequence",
      "state": "numeric.atom.start",
      "expectedContains": "integer",
      "found": "damage",
      "traceContainsState": "effect.damage.amount"
    }
  ]
}
```

Every populated diagnostic field must match the same emitted diagnostic. Omit
`abilityIndex` to match the card-level aggregate diagnostics.

## Fast loop

Use the repository wrapper to keep compilation and execution timings separate:

```powershell
# Once after parser code changes.
.\scripts\run-parser-fixture.ps1 -Build `
  -Fixture crates/deepdeck-parser-fixture-runner/fixtures/example.json

# Add or edit JSON fixtures, then run directly without Cargo.
.\scripts\run-parser-fixture.ps1 `
  -Fixture crates/deepdeck-parser-fixture-runner/fixtures/example.json
```

The binary accepts individual JSON files or directories and emits JSON Lines:
one `caseResult` per case, `fixtureError` for invalid inputs, and one `summary`.
Any failed case or invalid fixture produces a non-zero exit code.

## Parser worker stack

The runner executes fixture discovery, parsing, and assertion evaluation on a
dedicated worker thread with a 32 MiB stack. This matches the repository's
focused audit default and prevents deeply composed Oracle grammars from being
limited by the platform main-thread stack.

Set `DEEPDECK_PARSER_FIXTURE_STACK_BYTES` to override the worker stack for this
runner. When that variable is absent, the runner honors `RUST_MIN_STACK`; when
both are absent, it uses 33554432 bytes. Values must be positive decimal byte
counts.
