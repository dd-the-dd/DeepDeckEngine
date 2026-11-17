# Oracle checkpoint draft tools

Generate reviewable checkpoint drafts from an explicit pinned manifest. This
tool never accepts a checkpoint, installs a ledger, changes current documentation
or updates an authoritative pointer. ROOT review and explicit promotion remain
required. Every output retains `accepted:false` and `goalComplete:false`.

## Commands

Run from the repository root, using a new output directory for each invocation:

```powershell
python scripts/oracle_checkpoint/tool/collect_complete_source_snapshot.py --repo . --output .tmp/checkpoint-source-NEW --fixture-inputs .tmp/my-lot/fixture-hashes.json --additional-sources .tmp/my-lot/analyzer-source-hashes.json
python scripts/oracle_checkpoint/tool/checkpoint.py --repo . --manifest .tmp/my-lot/checkpoint-plan.json --output .tmp/checkpoint-draft-NEW
python -m unittest discover -s scripts/oracle_checkpoint/tests -v
```

Python uses only the standard library. PowerShell and the existing official
repository helpers supply semantic IR pins and runner fingerprints. Rust toolchain
version inspection is read-only; these tools never run Cargo or tests themselves
except the explicitly requested Python test command above.

## Manifest and capture order

`checkpoint-plan.schema.json` describes `deepdeck-checkpoint-plan/v1`.
The executable validators additionally enforce cross-file semantics.
All references use `{ "path": "repository/relative/path", "sha256": "UPPERCASE64HEX" }`.
Create explicit manifests; no historical certificate or task-specific `.tmp`
example is installed with this tool.

1. Pin the accepted parent certificate, complete source snapshot, fixture inputs,
   actual binaries and their official fingerprint metadata before running gates.
   The snapshot collector unions inputs from all three official parser, runtime
   and contract wrappers. Supply other analyzer/source inputs explicitly.
2. Run the existing gates and audits separately, retaining raw logs and wrapper
   timing reports. Record each artifact profile and its source/binary provenance.
   Capture source/fixture hashes before and after, not only after execution.
3. Declare actual expected gate counts; ordered before/after audits and exact raw
   input references; ten-set selection and separately deduplicated totals; prior
   and proposed proof ledgers; append/repin IDs and complete proof-fixture hashes.
4. Declare any exact rule rewrite by Oracle ID/index, pinned whole before/after
   rule, final status and reason. Carry forward every known correctness gap.
5. Validate to a new `.tmp` output, inspect `checkpoint-draft.json` and submit
   its evidence for ROOT review. Validation is not acceptance.

The root fields are schemaVersion, id, parent, sourceFreeze, sourceMode,
artifacts, gates, corpora, allowedIRChanges, setCorpora, setsExpected, ledger,
officialDigestScript, knownGaps and notes. Historical source mode additionally
requires historicalCertificate. Artifact profiles are runtime, parser, contract,
root-attested-analyzer and root-attested-gate. Gate formats are rust-test-log,
parser-log, contract-log, runtime-log, strict-audit and architecture.

## Guarantees and limits

- Counts come from actual positive harness results, DATA rows and assertions;
  passed flags alone, zero tests, fixtureError and inconsistent partitions fail.
  Interleaved named Rust lines are corroboration; harness summaries own counts.
- Prior native rules preserve case and every JSON type. Exact declared rewrites
  do not permit broad normalization. Oracle IDs, card/counter names and keys are
  never lowercased for comparison.
- Official PowerShell Get-SemanticIrDigest computes ledger pins. There is no
  competing Python semantic digest. Ordered ledger prefixes/specifications stay
  exact except explicitly declared pin-only changes and appended identities.
- Set occurrences and Oracle-ID dedup totals remain separate. The explicit dedup
  policy chooses the best-supported occurrence, never sums repeated cards.
- All official runner inputs must occur in the source snapshot. A selected
  checkpoint file count is not assumed complete. Missing historical source,
  fixture or raw-log hashes cannot be established retroactively.
- Output must be a new directory resolved inside repository `.tmp`; inputs stay
  unchanged. Hashes are rechecked after evidence reads. Source JSON requires UTF8;
  only actual PowerShell transcripts may use a UTF16 BOM.
- Exact ability text must be a substring of a raw source face. Normalized bullets,
  reminders or changed ability boundaries can fail closed; no normalization or
  unsupported boundary protocol is invented.
- Fresh official runtime/parser/contract artifacts are supported. A fresh release
  analyzer adapter is not implemented; analyzers currently require accepted ROOT
  attestation. Inheriting old CI3 evidence into a fresh epoch is not implemented.
  Neither limitation can be bypassed by calling historical evidence fresh.
- A blocking known gap prevents acceptance readiness. The tool never infers
  individual runtime behavior from parser IR or sets the migration goal complete.

Dedicated tests use only temporary files under `.tmp/checkpoint-test-sandboxes`,
including official-helper probes. They do not delete sources, caches or fixtures.
