# Card migration throughput

This report separates parser work, implementation, compilation, test execution,
and coordination for every token/FSM migration. The source of truth for an
individual card is its journal under `.tmp/card-migration-timings/`.

## Opt — priority 129

Opt is the first card measured end to end with the v1 journal. It was certified
as recognizer-native, primitive, contract-proven, and runtime-proven. The strict
migration prefix advanced from 128 to 129; Smothering Tithe is next.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 41m 16.8s |
| Analysis | 4m 41.7s |
| Implementation | 7m 34.3s |
| Focused tests | 5m 52.4s |
| Root review and certification | 13m 20.7s |
| Uninstrumented handoff/coordination gaps | 9m 47.7s |
| Compilation recorded by commands | 8m 00.1s |
| Actual test execution recorded by commands | 31.6s |
| Corrective loops | 3 |

The corrective loops were:

1. route the remaining generic Scry adapter through the token automaton and
   stop the legacy regex from claiming Scry;
2. apply formatting before the final formatting check;
3. update legacy generic-envelope regressions from the removed composite
   `scry` node to the seven primitive operations.

### Avoidable cost found

| Cause | Observed cost | Remedy |
| --- | ---: | --- |
| Strict release build run cold for one card | 210.5s wall, 195.1s compile | Run at a 10–20 card checkpoint |
| Incorrect exact filter ran zero tests | 157.2s wall, 135.0s compile | Reject zero-match runs and execute the discovered harness directly |
| Contract crate compiled a second parser copy | 60.1s wall, 58.3s compile | Fixed: the thin contract crate now re-exports `deepdeck-oracle` |
| Repeated parser lookups during analysis | Three runs / 23s execution | Resolve the card once by Oracle ID |
| Agent/root duplicate Cargo verification | Multiple rebuild/check cycles | One Cargo lane; root replays only one independent proof |

## Measured system throughput

The complete Reality Fracture audit took 252.0s when cold. Building consumed
227.3s, while parsing all 285 cards consumed 3.31s (about 86 cards/s). Without
the build, the whole audit path is about 24.7s. The parser itself is not the
current throughput bottleneck.

The certified priority audit at position 129 took 14.44s warm. Parsing 130
cards took 0.154s; contract and runtime proofs together took 0.076s. The global
architecture scan and script overhead consumed most of the remaining time, so
those checks belong at batch checkpoints too.

After removing the duplicate parser compilation, the representative City of
Traitors contract test took 5.67s after the structural change and 0.39s in an
independent warm run, down from roughly 55s before the change.

## Per-card working loop

1. Start `track-card-migration.ps1` before analysis.
2. Resolve the card once by Oracle ID and identify the smallest reusable missing
   automaton or primitive.
3. Implement, then format before compiling.
4. Run one exact parser target through `test-card-migration.ps1`.
5. Execute stable contract and runtime JSON fixtures directly.
6. Record every corrective loop and command, including whether compilation
   occurred and separate compilation from execution time.
7. Serialize Cargo work through one lane while other agents analyze or edit.
8. Run global architecture and optimized strict-prefix certification every
   10–20 cards, or when a shared primitive changes.

## Throughput targets

These are development targets, not guarantees:

- Already generalized card: 2–5 minutes.
- New parser concept using existing engine primitives: 10–18 minutes.
- New engine primitive: 20–45+ minutes.

Auditing an already supported set in five minutes is realistic. Migrating an
entire new set in five minutes is realistic only when its cards are already
covered by existing generic automata and primitives.

## Smothering Tithe — priority 130

The second measured migration used the improved workflow and advanced both the
recognizer and executable migration prefixes to 130. The next card is Brazen
Borrower // Petty Theft.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 21m 50.3s |
| Analysis | 4m 24.9s |
| Implementation | 7m 41.4s |
| Focused tests | 4m 45.2s |
| Root review and checkpoint certification | 2m 38.1s |
| Compilation recorded by commands | 3m 18.9s |
| Actual test execution recorded by commands | 27.2s |
| Corrective loops | 1 |

The migration replaced the card-specific
`opponentMayPayManaOrCreateToken` operation with the reusable primitive graph
`cardDrawn -> optionalPayCostPerformEffects -> payMana/createTokens`. Its one
corrective loop fixed a generic engine omission: card-draw triggers now carry
the drawing player as `triggeringPlayerId`.

Because that generic runtime change correctly invalidated existing proof
fingerprints, the checkpoint replayed 329 contract/runtime proofs. The replay
took 25.75s total; contract and runtime execution consumed 6.85s. This confirms
that data-driven proofs are cheap enough to replay safely after a shared engine
change.

The architecture guard now caches only zero-violation reports under an exact
content fingerprint. A measured full scan took 8.91s and a safe cache hit
2.04s. It recalculates the fingerprint after the scan and again after the full
audit to detect concurrent source changes.

## Brazen Borrower // Petty Theft — priority 131

Brazen Borrower advanced the strict native, primitive, and executable prefix
to 131. It added the generic `SOURCE can block only OBJECT_SELECTOR` automaton,
the reusable `HasKeyword` selector predicate, and effective-keyword runtime
matching for block restrictions.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 26m 15.2s |
| Analysis | 4m 01.3s |
| Implementation | 7m 03.7s |
| Focused tests | 7m 45.9s |
| Root review and certification | 3m 36.1s |
| Between-stage/coordination gaps | 3m 48.1s |
| Compilation recorded by commands | 3m 34.8s |
| Actual test execution recorded by commands | 14.6s |
| Corrective loops | 5 |

Two loops came from zero-match test filters, one from an incorrect preservation
expectation for the already-native Adventure face, one from a missing unit-test
import, and one from exhaustive matches after adding `HasKeyword`. The final
strict refresh verified 331 proofs in 23.60s. Because the effective-keyword
runtime matcher changed, that immediate proof refresh was required; the earlier
60.95s preproof was redundant and should not be repeated by the next card.

## Three-card bottleneck analysis

The first three complete journals cover 89m 22.3s of elapsed development time.
Their median is 26m 15.2s per card; the mean is 29m 47.4s because Opt includes
the initial infrastructure work and a cold release audit.

| Mutually exclusive wall-clock category | Total | Share |
| --- | ---: | ---: |
| Analysis | 13m 07.9s | 14.7% |
| Implementation | 22m 19.5s | 25.0% |
| Focused-test stage | 18m 23.5s | 20.6% |
| Root review/certification | 19m 34.9s | 21.9% |
| Between-stage/coordination gaps | 15m 56.5s | 17.8% |

Across the same wall clock, compilation accounted for 14m 53.7s (16.7%) and
actual test execution for only 1m 13.4s (1.4%). Compilation is material, but it
is not the dominant cause. Source analysis, implementation, review, duplicate
audits, and handoff gaps consume most of the elapsed time.

### Changes that reduce the next-card latency

1. Pipeline read-only analysis of card N+1 while card N owns the Cargo lane.
   The measured analysis phase is stable at 4m 01s–4m 42s, so overlapping it
   removes roughly four minutes from throughput without hiding development
   cost in the per-card journal.
2. Run exactly one final strict audit. Smothering Tithe and Brazen Borrower each
   ran a preproof and then a refresh; removing the redundant preproof saves the
   observed 84.87s and 60.95s respectively. Cards that do not change a shared
   runtime primitive wait for the 10–20-card checkpoint.
3. Format before the first compile and use an exact integration target plus a
   verified test name. Post-format rebuilds and zero-match diagnostics caused
   avoidable loops; zero-match failures are now explicit, but the filter should
   be obtained from the owned test before invoking Cargo.
4. Build each data-driven fixture runner once after the implementation settles,
   then execute all card/sibling/negative/runtime JSON fixtures directly. The
   measured fixture executions are sub-second; rebuilding between fixtures is
   the expensive part.
5. Keep root review independent but narrow: inspect the diff, replay one exact
   parser proof and direct fixtures, then perform the single required strict
   refresh/checkpoint. Do not repeat every agent-side unit test.

For a new grammar concept that reuses existing primitives, these changes put a
warm target of about 12–18 minutes within reach. Cards already covered by the
generalized automata remain a 2–5 minute classification-and-proof pass. New
runtime primitives can legitimately exceed that range because their proof
fingerprints invalidate the affected contract/runtime cache.

`track-card-migration.ps1 -Action report` now prints stage time,
between-stage time, non-command stage work, command wall time, unclassified
command work, and failed-command count. This makes the same bottleneck analysis
available immediately for every subsequent card instead of reconstructing it
manually.

## Fabled Passage — priority 132

Fabled Passage advanced the strict recognizer, primitive, and executable
prefix to 132. The new grammar composes an activated library search, a reusable
controlled-permanent-count condition, and a typed reference to the searched
collection. The specialized `resolveFabledPassage` operation and exact parser
branch were removed; the engine now receives only `chooseCards`, `moveCards`,
`shuffleZone`, `recordLibrarySearch`, `countPermanents`, `compare`,
`conditionalEffect`, and `untapPermanents`.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 26m 43.1s |
| Analysis | 5m 38.2s |
| Implementation | 14m 36.4s |
| Focused tests | 47.8s |
| Root review and certification | 5m 18.2s |
| Between-stage gaps | 22.5s |
| Compilation recorded by commands | 5m 20.6s |
| Test/command execution recorded | 2m 23.5s |
| Regenerable-cache cleanup within execution | 1m 44.0s |
| Corrective loops | 6 |

Only one loop corrected product grammar: the object selector initially owned
the comma that separates a condition from its effect. One loop corrected the
proof-manifest digest. The other four came from infrastructure: a workspace
member changed the lock, the D: volume filled, the first unsupported result
needed a structured diagnostic, and the external fallback target correctly
required an explicit trim opt-out.

The disk incident identified 30.07 GiB of regenerable Rust incremental state
under the isolated migration target. It was removed without deleting compiled
dependencies or sources, recovering 26.43 GiB of free space. The focused test
wrapper now trims only `debug/incremental` when that target exceeds 40 GiB, so
future card builds fail less often without forcing a fully cold dependency
build on every card.

After the authoritative digest correction, the final warm strict audit took
13.86s internally (14.97s wall), verified the four Fabled fixtures, and stopped
at the expected sentinel: Growth Spiral at priority 133. The earlier cold pass
spent 75.31s rebuilding after the cache cleanup; that is maintenance cost, not
parser execution.

## Stable parser fixture runner

`deepdeck-parser-fixture-runner` now gives the parser the same data-driven loop
already used for contract and runtime proofs. One strict JSON fixture can group
the exact card, generalized siblings, and negative near-misses; it validates
recognizer provenance, exact IR or JSON-pointer assertions, and structured
machine diagnostics.

The initial targeted build took 5.258s and its direct internal tests took
1.145s (3/3). After that build, a two-case fixture took 31.86ms inside the
runner and 0.211s in an independent end-to-end PowerShell replay, with no
Cargo invocation. Empty and malformed fixture inputs fail explicitly in about
12ms. Future card fixtures can therefore change without creating or relinking
a Rust integration-test binary; the runner is rebuilt only when parser source
changes.

## Growth Spiral — priority 133

Growth Spiral is the first card migrated with the stable parser-fixture runner.
It added the generic optional card-deployment automaton for
`PLAYER may put ARTICLE CARD_SELECTOR from PLAYER_ZONE onto the battlefield
[tapped]`. The lowering corrects the old decision timing by resolving
`chooseCards(0..1)` after the draw, followed by an independent `moveCards`.
No engine primitive was added.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 12m 53.4s |
| Parallel analysis on the critical path | 0.0s |
| Implementation | 9m 07.6s |
| Root review and certification | 3m 26.3s |
| Compilation recorded by commands | 1m 45.6s |
| Execution recorded by commands | 16.3s |
| Corrective loops | 3 |

The read-only analysis was completed in parallel while Fabled Passage owned
the Cargo lane, so its critical-path cost was zero; its agent labor predates
the Growth journal and is explicitly not presented as zero work. Two product
loops fixed exhaustive Rust matches and a no-candidate runtime expectation.
The third loop added the declared `audit-cli` feature to the root's digest
build command before compilation began.

After the one parser rebuild, exact card + sibling + near-miss replayed in
57ms without Cargo. Independent parser, contract, and four runtime fixtures
then replayed together in 154ms. The final strict audit certified migration
position 133 and discovered that position 134, Dovin's Veto, is already fully
native and primitive but lacks proof fixtures; the first recognizer-legacy card
is Kozilek's Command at position 135.

## Dovin's Veto — priority 134

Dovin's Veto required no parser or engine change. Both Oracle abilities were
already recognized by shared native automata: the stack-scoped
`cantBeCountered` modifier and the negated card-type selector used by
`counterSpell`. This pass added only data-driven parser, contract, and runtime
proofs, then registered the existing semantic digest in the proof manifest.

| Measurement | Time |
| --- | ---: |
| Productive staged work | 7m 50.9s |
| Analysis | 3m 31.6s |
| Data-only proof implementation | 1m 28.1s |
| Focused tests | 1m 17.0s |
| Independent root review and certification | 1m 34.1s |
| Direct root parser + contract + runtime replay | 174ms |
| Final strict-prefix audit | 23.6s wall |
| Cargo compilation for card fixtures | 0s |
| Corrective loops | 1 |

The single correction removed an overreaching synthetic spelling (`cannot`)
from a sibling fixture; the supported Oracle contraction (`can't`) and the
generic nonartifact selector remained covered. The final parser fixture passed
3/3, the executable contract passed 1/1, and the two runtime scenarios passed
11/11 assertions. An independent agent also found no card-name branch, regex,
or raw-text parser on Dovin's promoted path.

The journal's 5h 26m elapsed clock includes 5h 18m of suspended coordination
time while agent credits were unavailable; it is intentionally excluded from
the productive latency above. The strict audit advanced both recognizer and
migration prefixes to 134 and stopped at Kozilek's Command (#135), whose IR is
already primitive but whose recognizer is still legacy.

## Kozilek's Command — priority 135

Kozilek's Command replaced the legacy general-modal regex path with one
token-driven exact-cardinality modal parent and reusable children. The change
adds expressed target cardinality (`literal | X`), one shared `xValue`, scoped
mode decisions, targeted token creation with a typed quoted mana ability,
targeted scry expanded into primitive library operations, and both permanent
and graveyard-card exile children. No new engine operation was required.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 40m 30.2s |
| Analysis | 12m 47.1s |
| Implementation and focused proofs | 27m 42.8s |
| Compilation recorded by commands | 4m 36.4s |
| Execution recorded by commands | 27.7s |
| Final warm parser replay | 163ms wall |
| Three runtime fixtures together | 36ms wall |
| Final strict audit | 16.0s wall |
| Corrective loops | 4 |

The loops exposed reusable infrastructure and modeling issues: the fixture
runner needed a configurable 32 MiB worker stack; expressed cardinality had to
propagate through an existing activated-zone lowerer; a global target-player
reference created 25 illegal exhaustiveness obligations and was narrowed to
the modal child that owns its binding; and the initial generated-token runtime
proof was split because the runner validates action sources before execution.

The final parser fixture passed exact + generalized sibling + structured
near-miss 3/3. The contract was executable, and runtime passed 22/22 assertions
across modes 1–2, modes 3–4, and the embedded Spawn mana ability. A broader
automata partition left three unrelated baseline assertions red while every
affected modal/scry/token/cost/card-zone test passed. The strict audit advanced
the official migration prefix to 135 and the recognizer-native prefix to 137;
Stormcarved Coast (#136) and Blooming Marsh (#137) are already recognizer-native
but still require proof fixtures, while Phelia (#138) is the first mixed
recognizer.

## Stormcarved Coast — priority 136

Stormcarved Coast required no parser or engine change. Its two abilities were
already handled by the shared token automata for controlled-permanent-count
entry replacement and fixed mana choice, and they already lowered to primitive
IR. The migration added only data-driven parser, executable-contract, and
runtime proofs.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 6m 26.6s |
| Analysis | 17.5s |
| Parallel data-only proof implementation | 3m 11.0s |
| Independent root review and certification | 2m 57.9s |
| Direct fixture execution recorded by commands | 152ms |
| Final strict-prefix audit | 15.4s wall |
| Cargo compilation | 0s |
| Corrective loops | 0 |

The parser fixture passed exact + generalized sibling + structured near-miss
3/3, the contract passed 1/1, and runtime passed 21/21 assertions. Runtime
explicitly checks zero and one other land, the exact two-land boundary, and
both `{U}` / `{R}` mana choices. The final strict audit used five fresh proof
fixtures with zero builds, found zero architecture violations, and advanced
the official migration prefix to 136. Blooming Marsh (#137) is now the first
unmigrated card; Phelia (#138) remains the first non-native recognizer.

## Blooming Marsh — priority 137

Blooming Marsh initially appeared to be proof-only: its semantic recognizers
and primitive IR already existed. Independent review found that its end-to-end
path still executed the legacy regex/raw-text classifier before entering the
native automata. The migration therefore added a reusable token-only routing
layer for controlled-count entry replacements and `COSTS: Add ...` mana
abilities; the legacy classifier is now evaluated lazily only when this
promoted layer returns no hint.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 13m 55.4s |
| Analysis | 1m 11.2s |
| Parallel data-only proofs | 2m 6.5s |
| Review, classifier correction, and certification | 10m 37.5s |
| Compilation recorded by commands | 2m 9.7s |
| Execution recorded by commands | 1m 2.4s |
| Final warm strict audit | 15.9s wall |
| Corrective loops | 1 |

The parser passed exact + generalized sibling + structured near-miss 3/3,
the contract passed 1/1, and runtime passed 26/26 assertions across zero,
one, two, and three other lands plus both `{B}` / `{G}` choices. The zero-land
scenario also contains three opposing lands, proving that the count is scoped
to the source controller. The token classifier passed 8/8 tests, including
quoted/reminder colons, compound and parenthesized mana costs, a current-card
name link, malformed comparator commitment, and an unrelated-subject negative.

One transient Windows linker lock caused an LNK1104 retry; it was an
infrastructure failure, not a semantic failure. The final warm audit performed
zero builds, found zero architecture violations, and advanced both official
prefixes to 137. Phelia, Exuberant Shepherd (#138) is now the first gap.

## Phelia, Exuberant Shepherd — priority 138

Phelia replaced the legacy `exileUntilNextEndStep` composite with a native
token/FSM sequence and primitive IR: attack event, optional targeted exile,
successful-object binding, owner snapshot, delayed end-step trigger,
exile-scoped `moveCards`, returned-object binding, controller comparison, and
counter placement. The same change generalized target-slot `other` into typed
`excludeSource` metadata and routes trigger heads from lexical tokens before
the legacy classifier.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 26m 12.2s |
| Analysis and primitive design | 8m 50.6s |
| Parallel implementation and proofs | 17m 21.2s |
| Compilation recorded by commands | 8m 30.4s |
| Execution recorded by commands | 45.5s |
| Final warm strict audit | 16.3s wall |
| Corrective loops | 3 |

Three generic engine corrections were required: `singleBoundObject` now
resolves as a battlefield permanent when appropriate; `exilePermanent.bind`
contains only objects actually exiled; and optional `moveCards.from` constrains
both source zone and player. Four dedicated engine tests pass. The broader
engine partition left ten unrelated pre-existing assertions red while all four
new tests remained green.

The parser fixture passed exact + generalized sibling + structured near-miss
3/3, the two-rule contract (`Flash` plus the trigger) passed 1/1, and four
runtime fixtures passed 32/32 assertions: zero targets, controller-owned return
with a counter, opponent-owned return without a counter, and normal return when
the original source is absent. The source-zone mismatch is covered by the
generic Rust engine regression because the data runner cannot inject an
arbitrary move between exile and the delayed end step.

The first infrastructure loop corrected an engine-test feature selection; the
second corrected low-level Oracle test helpers after the production automaton
compiled; the third corrected generated instance suffixes and prevented an
opponent from losing to an empty library before the delayed trigger. The final
strict audit verified five fresh proofs with zero builds and advanced both
official prefixes to 138. Terminate (#139) is the next strict gap.

## Terminate — priority 139

Terminate replaced the legacy spell regex with a token/FSM composition of the
shared targeted-destroy child and a typed second-sentence regeneration
amendment. It lowers to the existing primitive `destroyPermanent` with the
independent `cannotRegenerate: true` modifier. A token-only spell hint routes
the family before the legacy classifier; malformed amendment verbs remain
claimed and produce structured diagnostics.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 13m 38.4s |
| Analysis | 3m 1.3s |
| Parallel implementation and proofs | 10m 36.8s |
| Compilation recorded by commands | 5m 11.3s |
| Execution recorded by commands | 32.4s |
| Final warm strict audit | 14.9s wall |
| Corrective loops | 1 |

The engine execution already handled regeneration prohibition correctly, so no
gameplay operation changed. Contract validation was tightened generically:
optional `destroyPermanent.cannotRegenerate` must now be Boolean. The dedicated
schema test accepts absence, `false`, and `true`, and rejects a string.

The parser passed exact + `nonartifact creature` sibling + structured
`restored` near-miss 3/3, and the contract passed 1/1. Two paired runtime
fixtures passed 16/16 assertions. The control case proves an installed
regeneration shield normally preserves and taps the creature; Terminate removes
the same shield, destroys the creature, and emits no regeneration event. The
only corrective loop fixed a test-only cursor import.

The final audit verified three fresh proofs with zero builds, found zero
architecture violations, advanced the migration prefix to 139, and found the
recognizer already native through 140. Arid Mesa (#140) is therefore the next
proof-only gap; Earthbender Ascension (#141) is the next legacy recognizer.

## Arid Mesa — priority 140

Arid Mesa reused the existing native fetch-land automata and primitive IR, but
independent review found that its promoted semantic path still executed the
legacy activation regex during classification. The migration extracted a
reusable token-only `COSTS : EFFECT_HEAD` scanner and added the effect head
`Search your library for`. The same scanner continues to serve mana `Add`
abilities while ignoring colons inside quotes and reminder parentheses.

| Measurement | Time |
| --- | ---: |
| End-to-end wall clock | 7m 58.4s |
| Analysis | 3m 52.1s |
| Parallel proofs and lexical correction | 4m 6.2s |
| Compilation recorded by commands | 52.6s |
| Execution recorded by commands | 26.3s |
| Final warm strict audit | 13.6s wall |
| Corrective loops | 1 |

The parser passed exact + Misty Rainforest sibling + structured near-miss 3/3,
and the contract passed 1/1. Four runtime fixtures passed 39/39 assertions:
independent Mountain and Plains choices, a legal empty search that still pays
all costs and shuffles, and an unavailable activation while the source is
tapped. Together they prove tap, life payment, source sacrifice, subtype-union
selection, untapped entry, shuffle/search events, and legality gating.

The sole corrective loop was architectural rather than semantic: native
recognizer provenance did not yet imply regex-free classification. The new
classifier suite passed 13/13 and its architecture guard passed 7/7. The first
strict audit rebuilt only the parser audit binary; the final warm audit used
zero builds and advanced both official prefixes to 140. Earthbender Ascension
(#141) is now the first gap.

## Earthbender Ascension — priority 141

Earthbender Ascension now composes two native token/FSM paths. Its entry
trigger lowers Earthbend into animation, haste, counters, a delayed zone-change
trigger, and an `earthbend` signal before the independent basic-land search,
move, shuffle, and search-record primitives. Its Landfall ability lowers an
opaque quest-counter placement, an actual-result binding, a separately stacked
reflexive trigger, a live source-counter threshold, a creature target, a
`+1/+1` counter, and a temporary trample grant.

| Measurement | Time |
| --- | ---: |
| Original migration tracker wall clock | 10h 16m 53.1s |
| Analysis and primitive design | 8m 56.1s |
| Coordinated implementation, review, and certification | 10h 7m 56.8s |
| Compilation recorded by the original tracker | 17m 54.2s |
| Execution recorded by the original tracker | 25.9s |
| Cold optimized strict pre-audit | 7m 9.2s wall |
| Final strict proof refresh | 1m 21.4s wall |
| Final warm strict recertification | 20.8s wall |
| Corrective loops | 3 |

The original tracker was already closed when review identified missing dynamic
event proofs, and it is intentionally left immutable. The following measures
were either captured outside that tracker during the original migration or
belong to the later proof-closure pass. They are reported as an addendum rather
than inserted into a fabricated chronology.

| Addendum measurement | Compile/link | Execution | Wall | Result |
| --- | ---: | ---: | ---: | --- |
| Earlier ability-2 test attempt omitted from the closed tracker | 78.092s | 0s | 78.092s | compile failure |
| Earlier corrected ability-2 suite (8/8) | ~69.0s | 0.03s | 69.796s | pass |
| Earlier classifier filter | 0.15s | 0.02s | 0.260s | pass |
| Earlier ability-2 parser fixture (4/4) | 21.623s | 0.636s | 22.402s | pass |
| Post-closure tightened architecture scan | 0s | 8.689s | 8.689s | pass |
| Post-closure warm strict audit, zero builds | 0s | 20.834s | 20.834s | pass |
| Proof closure: generic `playLand` test (1/1) | 2.671s | 1.315s | 3.986s | pass |
| Proof closure: first exact engine command | 41.800s | 0s | 41.860s | command configuration failure |
| Proof closure: corrected exact empty-antecedent engine test (1/1) | 199.668s | 1.112s | 200.780s | pass |
| Proof closure: runtime-runner rebuild | 0.307s | 0s | 0.307s | warm, no rebuilt artifact |
| Proof closure: six Earthbender runtime fixtures | 0s | 0.228s | 0.228s | 6/6 pass |
| Proof closure: architecture guard | 0s | 10.731s | 10.731s | zero violations |

The first proof-closure engine command used the focused wrapper's
`--no-default-features` mode. Shared partition-3 tests now import Oracle
compilation APIs, so compilation stopped before the Earthbender test ran. The
corrected command enabled `oracle-compilation,ci-unit-tests-3`; this is the one
corrective loop in the closure pass, not a semantic engine defect. Its complete
append-only timeline is
`.tmp/card-migration-timings/priority-141-earthbender-proof-closure.json`.

Combined review found one generic runtime contract mismatch: `putCounters`
binds the objects whose counters actually changed, while `boundValue` formerly
accepted only an integer binding. Numeric bound values now also expose object
collection cardinality. This makes the reflexive `When you do` gate true only
when the antecedent placement changed a live object, without adding a
card-specific operation.

The complete parser fixture passed the exact two-ability card, a generalized
artifact/charge/shield/flying sibling, and two committed structured near-misses
(4/4). The two-rule executable contract passed 1/1. Six runtime fixtures passed
46/46 assertions. Earthbender is actually cast and resolved, which proves its
ETB trigger is put on the stack before Earthbend and the tapped basic-land
search execute. A controlled land is played through the normal legal-action
path and proves Landfall from three to four with a separately stacked reflexive
ability; the same path proves that an opponent's land entry is ignored. The
remaining fixtures cover zero to one, intervening-if revalidation after the
quest count is reduced before resolution, and the exact
`compare(boundValue(Object[]), >, 0)` false path when counter placement changes
no live object.

Only after those proofs passed, both exact Landfall text branches and the
`advanceEarthbenderAscension` engine operation were removed. The architecture
guard reports zero violations (179/181 resolver-debt entries and 197/199
operation fields remain, both below baseline), and repository searches find no remaining
Earthbender-named branch, combined Earthbend operation, or
`resolveTriggeredInstruction` on the promoted path. The cold release parser
build cost 400.9s, while the 142-card parser audit itself took 0.69s; the final
cached parser audit took 0.18s. Strict certification advanced the native,
primitive, executable migration prefix to 141 with 376 proof records. Swan Song
(#142) is the next unsupported card.

## Swan Song -- priority 142

Swan Song now uses a token/FSM recognizer that composes the existing generic
counter-effect recognizer, descriptive creature-token characteristics, and
keyword-list recognizer. Its lowering snapshots the targeted spell's controller
before `counterSpell`, then independently executes `createTokens`. This preserves
the Oracle behavior when the spell cannot be countered: the spell remains on the
stack, but its controller still creates the Bird. No card-named, raw-text,
combined, or new engine operation was introduced.

| Measurement | Time |
| --- | ---: |
| Generic implementation | 6m 41.5s |
| Focused proof stage | 6m 19.5s |
| Cold optimized strict pre-audit | ~6m 55.0s wall |
| Final strict proof refresh | 32.26s wall |
| Compilation/linking recorded by commands | 9m 59.3s |
| Test and audit execution recorded by commands | 19.84s |
| Command wall time recorded | 11m 14.5s |
| Swan Song parse in final strict audit | 0.623ms |
| Corrective loops | 1 |

The cold pre-audit wall time is reconstructed from the release artifact and
report timestamps; its optimized parser build itself was reported as 6m 41s.
The final refresh was warm for the parser and separately measured 0.93s parser
build, 0.23s parser audit, 4.51s contract build, 2.83s contract tests, 1.14s
runtime build, and 5.34s runtime tests. The migration tracker includes a long
coordination wait while Earthbender Ascension proof closure stabilized the
shared manifest, so command time is the meaningful active-work measure.

The exact card, a generic red Drake sibling, and a structured near-miss passed
the parser suite 3/3 and the data-driven parser fixture 3/3. The primitive
contract passed 1/1. Two runtime fixtures prove both the ordinary counter path
and the counter-prevented path. The named-token counter sibling remained green
3/3 after the exact legacy Swan Song branch was removed, and the architecture
guard reports zero violations.

The sole corrective loop changed test expectations from legacy lowercase color
strings to canonical typed IR color names (`Blue` and `Red`); it did not change
the recognizer or runtime semantics. Strict certification verifies semantic IR
digest `DB5C33590A1E4900A07175ED47D7A6B87C69D5DF4CC3DD343EBBD24DB6C37E7B`,
advances both the recognizer and primitive executable migration prefixes to
142, and records 381 proofs. Esper Sentinel (#143), still routed through
`resolveTriggeredInstruction`, is the first remaining gap.

