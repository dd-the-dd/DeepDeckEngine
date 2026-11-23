# Oracle migration — pure nominal domain accepted; choice integration pending

## Session correction: global mission paused; focused shared query work

The user paused the global migration mission. Historical ACTIVE/live process
statements below are retained as prior evidence, not current execution status.
Do not resume the broad mission or the old conditional-program, checked-query,
or observed-cast-receipt drafts from this document.

The newly authorized focused task is shared predicates and Count(Query) for
cost reductions, preserving Foundry Inspector, Ruby Medallion, Oketra's Monument,
Ballyrush Banneret and Blasphemous Act, and extending Eddymurk Crab's reduction
clause. The full Crab remains unsupported until its other missing abilities are
proved. Include generic player scopes for zone holders, keeping zone holder,
object owner and ability controller distinct. Targets/choices need explicit typed
producers; selecting zones must not invent player targeting.

Root owns integration, docs and the single Cargo lane. New staged owners only:
generic_ast_coverage: .tmp/cost-query-parser-focused-20261115;
generic_trigger_events: .tmp/cost-query-runtime-focused-20261115;
cycle_proof_plan: .tmp/shared-cost-query-contract-peer-20261115 (review/fixtures).
Do not apply any old paused draft wholesale. No focused implementation is yet
accepted or certified. Timing: .tmp/card-migration-timings/shared-cost-player-query-20261115.json.

The previous nominal-choice scope correction was applied to MAIN but its updated
ten-test harness failed during LLVM output for insufficient disk; zero tests ran.
The parent eight tests passed on their earlier source. NTFS compression completed
and reclaimed space without deleting dependency artifacts or sources. Recheck
actual processes and fingerprints before any build; never call the ten tests green.

## Retired-kind precedent: proliferate (#65)

`retiredOracleKinds` in `scripts/card-migration-architecture-baseline.json` names
kinds, not implementations. `proliferate` was retired in #26 because its archived
form, `{kind, count}`, called the opaque `resolve_spell_instruction("proliferateOnce")`.
#27 reintroduced the kind as the CR 701.34a keyword action: the player-action
parser emits a typed `{kind: "proliferate", player}` node, and the engine executes
it natively as an arm of the counters domain (`effect_domains/counters.rs`). This
uses ordinary counter placement and a typed choice. #65 therefore removed the
stale entry. The opaque resolver no longer exists, and `resolveSpellInstruction`
stays forbidden.

A retired kind may return only on the same terms. It must be a CR keyword action
with its own typed IR fields. A shared domain validator and executor must handle
it as an arm, with no resolver or card-specific op. Parser-to-runtime proofs must
exist before its entry is removed. This is the precedent for `recruit`
(CR 701.70a) and `manifestDread` (CR 701.62a). Remove one entry with the commit
that adds the primitive and its proofs. Never clear the list wholesale.

Both carry a conditional part, and its runtime proof must exercise both branches.
A proof of the unconditional path alone does not qualify:
- `recruit` (CR 701.70a): draw, then discard. The 1/1 Human Soldier token is
  created only if the discarded card was a nonland card. Needs one runtime test
  that discards a nonland card (token created) and one that discards a land
  (no token).
- `manifestDread` (CR 701.62a): look at the top two cards and manifest one. The
  card that was not manifested goes to the graveyard. Needs a runtime test that
  checks both destinations: one face-down 2/2 on the battlefield and the other
  card in the graveyard, neither left in the library. Per CR 701.62b
  (checked against the cached rule text in `data/ability-catalog`), an ability
  that triggers whenever a player manifests dread triggers after the whole
  process is complete, even if some or all of its actions were impossible (for
  example, a library with fewer than two cards).

## Adventure layout runtime (CR 716, PR #69)

The Adventure part is linked to its card as an `adventureSpell` rule
(`src/model.rs`) and cast through `src/engine/adventure.rs`. An Omen part
still refuses, with its own state (`card-structure.omen-runtime`).

- Admission (CR 716.3a, 601.3e): the Adventure is offered from the hand, or
  from another zone only under a route that admits a card with the
  Adventure's characteristics: a static or temporary casting permission, a
  top-of-library permission (including the unfiltered top-of-library casting
  of this turn), or a permission for that one card. A permission
  that admitted the creature does not admit its Adventure, and the reverse.
- Resolution (CR 716.3d): the ordinary path exiles the card and grants
  `adventureCastPermission`. The permission is bound to that exile
  incarnation (`zoneIdentity`, CR 400.7), so it ends once the card leaves
  exile. A countered or fizzled Adventure goes to the graveyard.
- Cohort delta (probe, private target): league 824 -> 828 (+4: Brazen
  Borrower, Murderous Rider, Stormkeld Vanguard, Midgar), corpus 15,079 ->
  15,155 (+76), with 0 lost and 0 IR changes. 76 of the 167 Adventure cards
  are supported. Of the rest, 9 are Omen-refused and the others are blocked
  by ordinary grammar gaps.
- Proofs: `crates/deepdeck-runtime-fixture-runner/fixtures/adventure/`
  (resolution, the end to end path, counter, fizzle, admission in both
  directions, permission expiry, a sorcery Adventure on an enchantment, and
  a land adventurer played from its exile).
  `tests/playable_model.rs` pins Brazen Borrower's compiled rules to the end
  to end fixture.
- Permission scope: every admission of a single-card permission, the
  Adventure's included, goes through one check,
  `card_permission_in_scope`. It covers the permitted player, expiry,
  condition, controller-turn timing, `whileSourceControlled`, the
  `azulaExiledPermission` source, `whileInZone` and the bound incarnation.
  Impulse grants (`grantCardPermission`, `grantPermission`) are bound to the
  card's incarnation when granted. A zone change that creates a new object
  ends every permission confined to the zone the card left (CR 400.7).
- Known gaps, failing closed: exile is scanned only for the player's own
  cards, so an Adventure on another player's exiled card is never offered.
  Not yet proved: a copied Adventure (CR 716.3c, 707.10), a controller who is
  not the owner, and an Adventure that leaves the stack by any means other
  than a counter or fizzle.
- `card-structure.layout-runtime` is now `card-structure.omen-runtime`. The
  diagnostic code `card_layout_runtime_missing` is unchanged. The only other
  reference to the old state is the historical count table in
  `docs/oracle-full-migration-plan.md`.
- Replay delta for the Adventure parts whose status changed:
  `docs/migration-baselines/adventure-runtime-2026-11-23.md`.

## Historical handoff (superseded status and owners)

Goal ACTIVE and unfinished: every Magic card through generic compositional typed
Oracle AST, shared primitive IR and correct runtime. No complete set certified.
Doctrine: oracle-compositional-compiler.md. Runtime capability/module loader remains
a design target in oracle-runtime-capabilities.md, not an implemented speedup.

## Latest accepted pure boundary

.tmp/nominal-domain-root-20261115/certificate.json
SHA F49F20F8FC18E6DE1700EED27531BAF2356C7D181A49515843D0CEDB3017466E.
New pure deepdeck-card-domain crate, no parser/runtime dependency. Source989 snapshot
4D05B650E902AF5A6D8AFCCD6C76F443AC19FBC194478E930F1112C4C6DF8FA7.
Immutable archive991 includes nominal integration test + focused wrapper,980 links
ONLY to immutable parent archive and11 copied files. Receipt49BBB6A5. Never edit
linked archives. No new cards counted; ledger211 unchanged.

Core freeze1345A108/manifest82269C3D:367Creature +147noncreature identities from
existing registries,5 witnessed apostrophe aliases,514identities/691surfaces verified
without collision by peer. Binary lookup reuses a OnceLock registry; successful
query lookup allocates nothing after initialization. Full printed nominal parsing
preserves hyphens/apostrophes/Time Lord, correlates families incl Kindred, and returns
explicit unknown/Plane/Dungeon/composite errors. This is not effective layering.
Same ID/family service is intended for parser and runtime; their consumers remain
separate work. Noncreature JSON147 is installed byte-exact from6F03, its10OracleRust
grammar changes are NOT applied.

Actual domain integration target nominal9/9 PASS: wrapper compile9.031s/run0.330s,
total15.096s incl cache scan5.406s. Executable pinned and replayed9/9. Installed
fingerprint test11roles/5dependency mutation-restores/11missing-manifest rejections,
audit script cache tests and runtime wrapper smoke PASS. Python33 passed via actual
direct subprocess exit0; first PS stderr-transport odd exit1 with33/OK is retained.
Actual architecture0violations/0warnings. No new global/set/gameplay replay claimed.

Fingerprint freeze84D23D5B/manifestEA256CD4 is ALREADY MAIN:3wrappers,3audit input
lists,architecture fingerprint and affected tests close domain/data dependencies.
Runtime intentionally tracks complete oracle-vocabulary root. Runtime test imports
production enumeration, fixing observed former red first-fresh-metadata assertion.
DomainTest/DomainTestTarget added to focused wrapper; exact target lib|integration
reuses single Cargo lane and separate compilation/execution timing.

## Current MAIN candidate — NOT yet accepted

.tmp/nominal-choices-root-20261115/application.json: exact choices8C0C3926 +
path-companion094FDE79,24pins/12RAW,4Rust +runtime Cargo dependency +lock. ALL ALREADY
MAIN; never reapply parent full files. Stable complete367Creature options replace
world-derived lists in both entry and resolution; explicit options restrict and
validate. Entry checks constant options and only PRESENT incoming bindings before
mutation; aliases canonicalize, ambiguous binding reuse errors. Existing payment/
condition order preserved; inactive-rule prompting before condition is separate debt.
Peer review found a BLOCKER beyond those tests: duplicate decision IDs within one
entry rule can skip a later nominal producer and reuse an out-of-set/wrong-family
value even when initial bindings are empty. Cycle owns a STAGED per-rule shared
scope validator/contract+runtime negatives. Separate rules keep independent scopes.
Do not accept the candidate even if the original8 pass before this fix is applied.
Eight dedicated CI3 tests proposed incl real entry/provider/store and public
instruction callback/event; no fake147 membership claim.

First focused runtime session52561 TERMINAL FAIL: compile218.45s/run0, query-cache
write os112 disk full, not a failing Rust semantic test. During this owned lane,
official40GiB cache maintenance removed one OLD nonworking incremental group2.48GiB,
pinned/rechecked active working group, touched no deps/sources/unrelated target.
Receipt cache-recovery-during-build.json. First source snapshot write also failed;
preserve that partial directory. Same unchanged source second focused build59493
was last verified LIVE; re-poll that handle/process, never restart on observation
timeout. Source retry02 session40356 TERMINAL PASS:991input snapshotC7BC2FCC under
frozen-source-02. Check actual state after handoff; no second Cargo lane.

Next after focused proof: inspect failures, then relevant fullCI3/runtime DATA/
contract regressions and optimized strict145 +architecture after shared runtime
change. No acceptance until actual results. Future normal runtime gate main+four
historical declaration probes1187cases/8111assertions;24 anyzone DATA are durable.
Omit their staging copies. Parser16target cases and contract48gain cases durable;
keep13 external Lantern parser cases and four historical contract gain directories.

## Last accepted engine/card checkpoint (historical source, not new certificate)

.tmp/anyzone-coherent-root-20261115/certificate.json SHA5B8D7094; ledger211C6101BF1.
Hall of Heliod's Generosity alone newly gameplay-certified, fullOracle/two rules,
four runtime scenarios and strict1/5proofs; independent751reference closure.
Old210 rows/raw prefix preserved. Source983 archive1F1A8217 retained unchanged.
Last CI3 501 PASS; runtime1163+24 PASS on same frozen profile, logical union1187/
8111, not a separate combined invocation. Contracts1390 PASS; architecture0/0.
Strict145 still fails at Rest in Peace42:41recognized/0refreshed proofs/prefix0.
Do not call41recognitions gameplay certificates or apply that old source proof to
new candidate. Hall root timing closed; elapsed5846.308s includes discussion/review/
orchestration/bookkeeping. Measured22commands compile531.153s/execution250.270s.

Retain bounded fixed200debug performance signal: baseline8.220s, first13.227s,
final9.714s,0compile each. One end-to-end observation per revision includes setup/
assertions/process; no causal/statistical/pure-simulation speedup claim.

## Parser / cohort / owners

Parser source unchanged C1:1190Native+1190Full fixtures;39288global5154complete
nonempty/20425native abilities,48gains/72abilities and20353oldNativeIR exact.
Ten local Nov13 Native set occurrences393/2368, unique346/2319. These are parser
metrics, not gameplay. ScopeCheckedAbilityBody remains partial, family/compat paths
remain. New combinations still require shared contracts, no regex-free recipes.

Cohort27 adoptedE215E921/input0E8AFB6F:100cards0complete/27partial/73none,34Native.
Hall removed,Land Tax added rank87/global246;99fullraw/order/provenance retained.
Quotas34/33/33,cursors251/266/278 unchanged. Root verified25artifact+983oldsource pins
and all750raw/faces/cached outputs. Advance3.076s,new100audit.605s,no750rerun.
Backend snapshots26+47 reconcile73unique unresolved,no overlap/ledger resolution.
root-failure-contract-inventory.json records173failedabilities/38stopping buckets;
these are NOT semantic fix clusters or predicted complete-card gains.

Root owns MAIN/Cargo/manifests/lock/review/integration. Agents STAGE ONLY:
- generic_ast_coverage: frontend E89CEB2C/manifest36094499 frozen,10OracleRust/24RAW
  composition replacing2wrappers and preserving8old6F03 candidates. Supplemental
  handoff82DD7C8C (original HANDOFF empty due disk event; freeze immutable).
  Do NOT admit broad parser147 before checked runtime roles. Next read-only task
  ranks generic fixes on actual cohort27 by whole-card blockers/caller evidence.
- generic_trigger_events: checked nominal query/target design46348 approved for
  coherent staging. Prepared params/facts reuse Boolean spine; add Evaluation
  error transport and honest legacy Result<Option> adapters. Actual source map
  overrides old summary claiming nonexistent Engine enum variant. Expanded role
  includes declaration/target numeric/destruction and mechanical legacy adapters.
  No partialResult fallback, fakeStackObject, global constructor invariant or
  persistent world cache. Costs/triggers/native mana/public infallible APIs remain
  explicitly unclosed; no universal147/layers claim. No Rust of that role yet proven.
- cycle_proof_plan: pure static peerGO complete; now read-only choices review.

Mulligan originalBAC/rebaseD31C remain UNAPPLIED,3 invariants unexecuted. ArbitraryN
ordering still n6 explicitcap; actual resolving-spell logical Stack lifetime remains
debt. Do not delete unrelated work,reset/commit,format archives or mark goal complete.
Nominal concept timing .tmp/card-migration-timings/nominal-subtype-domain-root-20261115.json
has closed pure phase and open runtime-choice phase; wall includes orchestration.
