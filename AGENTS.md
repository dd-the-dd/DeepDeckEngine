# DeepDeck engineering rules

These rules apply to every agent working in this repository.

## Mission and migration loop

- The objective is to parse every Magic card and execute its rules correctly with
  DeepDeck's generic Oracle language engine. Follow `docs/oracle-migration-objective.md`.
- Read `docs/oracle-migration-current.md` for the current cohort, owners, proof
  snapshot and pending work before restarting migration work after a handoff.
- Alternate recent local sets, the next unmigrated priority-queue card, and cards
  actually played in public DeepDeckLeague decks. Preserve each source's order,
  cursor and provenance; deduplicate by `oracle_id`. Keep the historical strict
  priority prefix as an independent regression suite.
- For each lot: audit, cluster failures, analyze generic fixes, assign non-overlapping
  agent ownership, implement shared automata, review integration, prove primitive
  runtime behavior, replay affected corpora, report coverage/timing deltas, repeat.
- Analyze up to 100 currently uncovered cards drawn from the rotating sources.
  Group their blockers by shared grammar concept and rank generic fixes by the
  number of complete cards they can unlock. Assign the highest-impact fix to an
  agent, prove the parser-to-runtime path, and replay the same cohort plus regressions.
  Refill the cohort and repeat. This supersedes first-blocker implementation order;
  the historical priority prefix remains a regression metric, not a scheduling gate.
- Use `scripts/advance-oracle-migration-batch.ps1` to retain unresolved cards and
  refill the cohort. Its queue, audit and Oracle inputs must describe the same
  ordered identities. Preserve its UTF-8 checks and input hashes across replays.
- Prefer local data and cached public snapshots; import only missing information.
- A stat bonus is an ordinary action in the shared effect sequence. Child actions
  return typed nodes and cursor positions; parents own full consumption and bindings.
- Preserve tests with distinct semantic value. Prefer data-driven variants, isolate
  legacy suites from the fast loop, and remove only superseded or unreachable tests.
- Do not delete unrelated agent, clock, training or backend work during migration cleanup.

## Compositional compiler contract

- Before changing Oracle grammar, IR or runtime coverage, read
  `docs/oracle-compositional-compiler.md`. It is the durable architecture and
  iteration contract; `docs/oracle-migration-current.md` records current facts.
- Develop a language capability, using a card as evidence. A new combination of
  already supported concepts must require no new production code. Diagnose shared
  composition, typing or binding failures before introducing another family parent.
- Regex-free recognition is not proof of compositionality. Reject a recipe-specific
  recognizer even if it has no card-name branch; keep predicates, numeric expressions,
  references, actions and program control independently reusable.
- Before asserting that a capability is absent, read the complete implementation
  and relevant callers. Preserve distinct grammar, dependency and runtime errors.
- Include a previously unused combination of supported concepts in acceptance
  evidence. Only compositional fixes enter the existing impact-ranked scheduling
  loop; report abilities and no-code combinations separately from complete cards.
- For new sets/keywords, follow the versioned release workflow in the contract:
  baseline first, authoritative rule definition, typed expansion or intrinsic rule,
  correct event policy, then unchanged-set and affected-corpus replay. Vocabulary
  recognition or a declared template is not runtime coverage.

## Architecture

- Compile Oracle text into small, reusable IR primitives. Do not add card-named, collector-number-specific, full-sentence, or combined-effect operations.
- Compose independent concepts (`dealDamage`, then `gainLife`) instead of adding combined concepts (`dealAndGainLife`).
- Put shared target, condition, numeric-expression, duration, event, and zone logic behind one generic implementation per concept.
- After the face/part factory, establish logical ability boundaries before
  tokenizing and parsing each ability. Preserve modal continuations and nested
  text; a newline alone does not prove an independent ability.
- Recognize the structural introducer first, then fill typed frames with child
  results. A triggered ability owns an event pattern, optional conditions and an
  effect program. Event actors, referenced objects, object predicates and origin
  zones are separate attributes; use symbolic bindings during parsing and resolve
  game objects/players during execution. Never find verbs globally or infer the
  scope of a connector without its enclosing frame.
- Keep parser, parser/runtime contracts, and engine runtime as separate compilation boundaries.
- Prefer small domain modules with narrow interfaces. Do not add new responsibilities to `src/engine/mod.rs` when a domain module is appropriate.
- Reuse `src/engine/mana.rs` for mana-cost parsing and transformations.
- Keep total-cost determination separate from payment substitutions: selected
  base/alternative plus increases, then reductions, then total-cost floors.
  Delve, convoke and improvise apply after that total is determined; do not put
  Delve into the reduction sum before a floor. See cached CR 601.2f and 702.66b.

## Card coverage

- Start every card implementation with `plan_card_structure` in
  `src/oracle/card_structure.rs`. Preserve complete ordered parts and metadata.
  Resolve `card_face_factory_missing` (unknown layout or alternative profile)
  or `card_face_structure_invalid` before extending ability grammar. A typed
  structural plan is not an executable-card coverage claim. Follow the
  `deepdeck-card-structure` skill for this preflight; never flatten unknown parts.
- Prove the complete path: Oracle text -> canonical primitive IR -> executable runtime behavior.
- A parser result backed by `resolveTriggeredInstruction` or another card-specific operation is migration debt, not primitive coverage.
- Add a positive card test, a generalized sibling-pattern test, a negative near-miss test, and a runtime behavior test when the concept executes in the engine.
- Surface unsupported grammar explicitly. Do not silently downgrade, approximate, or accept partially parsed abilities.
- Validate numeric bindings after card composition in the correct face context.
  A syntactically recognized `decisionResult(xValue)` is not executable without
  a declaration or the same spell face's implicit mana-X declaration. Never
  borrow a variable from another face or substitute zero for a missing producer.
- Coordinated object heads and their shared trailing qualifiers are different
  from separately qualified branches. Preserve both scopes; do not hoist a
  qualifier across a branch that already has a local qualifier or controller.
- Relative characteristic clauses such as `that's red or green` belong to the
  shared object-query grammar. Effect, event, and cost parents must receive the
  composed predicate instead of reimplementing that relative clause separately.
  Preserve the caller's comma boundary and reject an extra controller relation.
- Numeric properties of a parsed mana cost belong to `ManaCostNode`. Return a
  bounded typed amount from its preserved symbols; do not concatenate an Oracle
  mana-cost string and reparse it in an effect or static-ability parent.
- An atomic family claim must not intercept conditional or multi-sentence
  families. Check the complete token envelope, including any supported reminder,
  and include a Full-mode compatibility probe when widening a native claim.
- Cursor advancement is not a semantic commit. A child can consume a lexical
  prefix before discovering unsupported grammar; parents must use an explicit
  transactional outcome instead of a token-index threshold to claim that child.
- Distinguish replacing destruction from replacing the resulting zone change.
  Regeneration/shield/Umbra prevent destruction; an exile replacement for the
  graveyard destination can preserve destruction without producing a death.

## Tests and source layout

- Tests must live in dedicated test files or test directories, not inline in production modules.
- Split large test files by domain as production modules are extracted.
- Follow the test tiers in `docs/oracle-compositional-compiler.md`. The local edit
  loop runs focused units/DATA; feature completion runs the meaningful advanced
  gates. Do not replay every heavyweight suite after every edit.
- Add tests for distinct semantic risks, not one Rust unit per card/value or tests
  mirroring implementation. Reuse fixtures/parameterized cases and existing proofs
  when they cover the same invariant. Preserve the real card's end-to-end evidence.
- Full-engine scenarios and combinatorial/resource-limit probes are integration
  or robustness proofs, even under `--lib`. Keep them in an explicit completion
  gate; do not delete or silently ignore them to advertise a faster unit loop.
- Run the smallest parser, contract, and runtime partitions that prove the change, then run the relevant regression gate.
- Report compile time separately from test execution and parser audit time.

## Parallel work and integration

- Do not commit, merge, reset, or discard shared-worktree changes unless the root agent explicitly asks.
- Before editing, state the files and responsibility owned by the task. Avoid files owned by another active agent.
- A handoff must list changed files, tests and results, known risks, and interactions with other in-flight work.
- Agent output is not integrated until the root agent reviews the diff and announces acceptance to the team.
- After an accepted architectural change, reuse the accepted boundary and do not duplicate its implementation.
- Keep shared parser files compilable while another accepted change awaits a
  regression rebuild. Prepare incomplete cross-module changes in a task staging
  directory and apply the complete patch together after the root releases that
  boundary. Never restore a dirty shared file from Git to park another task.

## Migration throughput

- Start `scripts/track-card-migration.ps1` before analyzing each new queue card. Record analysis,
  implementation, every corrective `edit -> proof -> diagnosis` loop, Cargo compilation/link time,
  test execution time, and the final strict audit. A card handoff must include the timing JSON path.
- Use `scripts/test-card-migration.ps1` with an exact `*TestTarget` whenever a Rust harness is needed;
  preserve its separate compile and execution measurements. Prefer stable data-driven fixture runners.
- For card-level parser proof loops, prefer `scripts/run-parser-fixture.ps1`: build the stable runner
  once after parser source changes, then add exact-card, sibling, and near-miss JSON cases without
  creating or relinking another Rust integration-test binary.
- Validate fixture deserialization, not only JSON syntax. In particular, a failure
  expectation requires its `diagnostics` field. Copy the established fixture
  schema and use the existing runner to detect `fixtureError` before treating a
  new case as ready; an old parser's expected grammar failures are not coverage
  proofs, but its unchanged fixture schema can still catch malformed data.
- Distinguish exact IR equality from semantic equivalence. PowerShell `-eq`
  compares strings without case by default; use a case-sensitive structural
  comparison for JSON proofs. Any equivalence normalization must name the
  precise primitive and field (for example known color predicate values), not
  lowercase arbitrary JSON strings, IDs, keys, or card/counter names.
- Derive report totals from their per-set rows and verify the partition sum.
  Deduplicate by oracle_id across sets separately from per-set occurrences.
- Format changed Rust files before freezing the build input. For targeted
  rustfmt calls use `--edition 2024 --config skip_children=true`; do not format
  an entire dirty module tree while preparing a small migration.
- Windows directory listings can show stale sizes for files still open by an
  analyzer. Read the file or inspect actual progress before diagnosing a stall;
  retain completed rows with an explicit bounded manifest after an interruption.
- Use cached, set-level or queue-level audits instead of recompiling for every card.
- Keep clause-removal/readiness probes separate from real-card inputs and mark
  their mutations explicitly. Before a vertical card proof, compare its complete
  semantic input against the current cohort snapshot and record the Oracle-text
  and input-file hashes. A matching card name or oracle_id does not prove that a
  probe still contains the real Oracle text.
- Reuse `.tmp/targets/card-migration` for contract and runtime proof builds. Cargo already
  serializes writers to one target; per-agent runtime targets duplicate the large engine
  compilation. Use a separate target only when the task explicitly tests cache isolation.
- The focused migration wrapper trims only the regenerable `debug/incremental` cache when its
  isolated target exceeds 40 GiB; do not delete `deps`, fixtures, sources, or unrelated targets.
- Keep one active Cargo lane. Other agents may analyze and edit while it is occupied, but must
  coordinate before compiling so queue waits are not misreported as development or test time.
- Serialize workspace-manifest and lockfile edits through that same lane; changing workspace
  membership while another card is about to compile invalidates `--locked` even before Rust runs.
- Run optimized strict-prefix and global architecture certification at a 10–20 card checkpoint,
  or immediately after a shared runtime primitive changes. Per-card loops use exact parser tests
  and direct data-driven contract/runtime fixtures.
- Parser release artifacts may remain target-keyed and isolated because
  `scripts/audit-card-set.ps1` fingerprints and verifies them independently.
- Migration queues may be ordered by current play frequency across formats; identify cards by `oracle_id` when possible and by set plus collector number otherwise.
- The five-minute target applies to a warm audit/classification pass. New generic grammar and runtime behavior still require complete correctness proofs.
