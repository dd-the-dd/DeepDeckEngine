# Link native Oracle output to complete-source runtime fixtures

`scripts/link-native-oracle-fixtures.py` replaces per-card Python linking scripts for the supported card structure. It runs a pinned native Oracle analyzer on one unchanged source card, checks an explicit complete IR contract, and produces linked copies of runtime DATA. It never runs runtime tests or certifies card migration.

## Supported input boundary

- `--source-input`: a UTF-8 JSON array of complete Scryfall-style source objects, such as the current cohort or local global input. Exactly one object's **case-sensitive** `oracle_id` must equal `--oracle-id`.
- First version: **`layout=normal`, no `card_faces`** (absent, null or empty array). A different layout or nonempty parts produces `tool_layout_unsupported`, explicitly a **linker capability limit**, not `card_face_factory_missing`. Malformed parts metadata is separately rejected. Nothing is flattened or inferred.
- The real parser still owns card-structure factory validation and grammar. The tool requires one canonical audit row with a nonempty complete set of canonical/native abilities and no ability diagnostics.
- `--expected-rules`: a JSON array containing **all complete rules in order**, before fixture-specific selection. Every actual rule must match structurally and in case/type. `true != 1`, `1 != 1.0`, and names/counters/IDs are never lowercased. JSON object member order is immaterial; arrays and their order are not.
- `--fixture`: files or directories containing `deepdeck-runtime-fixture/v1` JSON. The flag can repeat; each flag accepts multiple paths. Directory traversal selects JSON files recursively. Non-fixture JSON and duplicate paths/IDs fail rather than being ignored.
- `--source-instance-id`: the source's **fixture definition alias**, usually `executor`, in `initialState.players[*]` card zones. Despite the flag's historical name, it is the `FixtureCard.id` / `sourceDefinitionId` alias, not a generated runtime physical instance ID. Supported initial zones follow the existing DATA schema: library, hand, battlefield, graveyard and commandZone.

Every fixture needs a physical source alias in those zones. Every matching source copy must preserve the real name, typeLine, manaCost, power and toughness and **all actual rules in order**. Optional absent P/T and explicit null both mean the source has no characteristic. A partial rule list, wrong metadata, or a source-shaped object in unused metadata cannot satisfy this requirement. Other fixture cards, such as a responding opponent, are not overwritten.

The top-level fixture `rule` must be exactly one complete actual rule. Any `executeRule.rule` override in nested actions, including a `probeFailure` action, must also equal a complete actual rule. Arbitrary shortened programs and combined substitutes are rejected. The tool injects copies of the actual emission into those validated positions and every validated source copy; it does not repair an incorrect fixture to make it pass.

## CLI

```powershell
python scripts/link-native-oracle-fixtures.py `
  --source-input .tmp/oracle-global-input-39288.json `
  --expected-source-input-sha 47DD40FBDDF789A4AFF07AE27DCC3775AAE76FFF962C58E66C2C5DA05640EC02 `
  --oracle-id 22f1a4a4-c423-4d1c-8775-0ed604a9fa51 `
  --analyzer .tmp/night-deathrite-root-01/after-analyzer.exe `
  --expected-analyzer-sha 04CA8E6D09B201CE0BAC721B075E0B8583386EF6B6B0A6BAA325F98052565944 `
  --expected-rules .tmp/night-deathrite-new13-contracts-01/actual-rules/22f1a4a4-c423-4d1c-8775-0ed604a9fa51.json `
  --fixture .tmp/night-deathrite-root-01/actual-link/runtime-complete-card `
  --source-instance-id executor `
  --output .tmp/a-new-reviewed-proof-directory
```

All listed options except `--expected-source-input-sha` are required. The optional source pin is strongly recommended when a reviewed cohort/source hash exists: it detects a source already modified before the command starts. Without it, the supplied file is the source authority; the tool does not independently authenticate it against Scryfall or a previous snapshot.

`--output` must not exist. Repeating the command requires a new output directory; no previous evidence is deleted or overwritten. Source, expected-rules, analyzer and fixture hashes are checked before/after. The selected source is serialized as a one-element array preserving its complete structure and values, hashed separately, and also checked unchanged after analyzer execution. The analyzer receives `DEEPDECK_ORACLE_NATIVE_ONLY=1`. No Cargo or runner build is invoked.

## Output schema

The new directory contains:

- `exact-selected-input.json`: complete one-card source, with its independent byte hash;
- `actual-native.jsonl`, `analyzer.stdout`, `analyzer.stderr`: raw analyzer evidence;
- `actual-rules.json`: complete ordered actual rule list, preserving names and value types;
- `runtime/NNN-original-name.json`: linked DATA copies, retaining fixture IDs and assertions;
- `link-manifest.json`, schema `deepdeck-native-fixture-link/v1`.

The manifest includes `oracleId`, `cardName`, `layout`, `sourceAlias`, `completeRuleCount`, every source/tool/analyzer/expected/native-output hash, per-fixture source/output paths and hashes, selected actual rule indexes, exact source-copy locations, nested executeRule overrides, and separate analyzer/total wall times. `inputHashesStable=true` and `caseAndTypeExact=true` record the checks performed. It always states:

```json
{
  "status": "LINKED_NOT_RUNTIME_EXECUTED",
  "runtimeExecuted": false,
  "coverageCertified": false,
  "cargoExecuted": false
}
```

All fixtures are validated before the tool emits any linked runtime file. On a failure, CLI exit code is1 and stderr contains a JSON error code/message with `runtimeExecuted=false`. Analyzer and selected-input evidence can remain for diagnosis; there is no success manifest or partially linked runtime lot.

## Proof limits and tests

Linking proves input identity, native emission and strict fixture/source correspondence. It does **not** deserialize the complete DATA schema through Rust, run or select runtime branches, prove target legality/SBA/trigger timing, certify static engine executability, generate official semantic IR pins, or promote a queue entry. Root's existing contract/runtime/strict gates remain necessary. A source alias may occur in several zones/copies; the tool validates all metadata but does not decide whether an execution source is ambiguous or an action is legal.

The supported normal/single boundary is deliberate. Multifaces, split, Adventure/Omen/prepare parts, empty rule sets and synthetic source metadata need a separately reviewed extension. Do not work around a tool limit by editing a real source layout, stripping Oracle text or changing its face metadata.

```powershell
python scripts/tests/test_link_native_oracle_fixtures.py
```

Dedicated tests use mocked analyzer execution for integrity failures. The staging validation also links the eight real accepted04CA Deathrite fixtures to its three complete native rules through the pinned real analyzer. That integration check creates DATA copies only; those eight runtime simulations are not rerun by the linker.
