# Parameterized Ward and collection keyword grants — 2026-11-14

ROOT accepted this checkpoint; the migration goal remains active. Certificate:
`.tmp/night-ward-root-01/certificate.json`, SHA256
`7EDE9F3B846AD7E991B91292D9D3391790039A358CC2460B86A750488542A9AC`.
Its accepted source freeze pins 844 inputs, SHA256
`DE4D513D0F1FF291DCF0D286E1E4773506B78E1CBC2E6916E72AF0FB5CD9B284`.
Parent is the accepted source837 ColorSet/destruction checkpoint. Analyzer SHA256:
`3CAD4477850FC19C78EDAF3E4168EC77186FC9BDEAE0CD3B307D5B7296EC15BB`.

## Shared language boundary

Ward consumes the shared typed resource-payment child: bounded fixed mana or
literal nonnegative life, with the targeting player as payer. Activated costs
reuse that child while keeping their existing controller binding and IR.
Reuse this accepted boundary for future resource keyword costs; do not introduce
a second Ward/payment parser. Unsupported resources and cost expressions fail
explicitly. Mana symbols use the preserved ManaCostNode property rather than
string concatenation and reparsing.

Collection grants compose the object query, keyword list and persistent grant.
`Other` excludes the source object by identity, including two sources sharing a
name. Quoted keyword-resource recognition is scoped to the grant envelope; no
global vocabulary exception was added. Typed outcomes own commitment. Conditional,
multi-sentence and unsupported envelopes are not claimed through diagnostic text
or cursor advancement. Existing native keyword IR remains exact.

## Proofs

| Gate | Result | Evidence scope |
|---|---:|---|
| Parser units | 983 | Fresh |
| Parser Native / Full DATA | 729 / 729 | Fresh |
| Runtime DATA | 839 / 5688 assertions | Fresh execution, stable runner |
| Contracts | 428 | Fresh |
| Hexing Squelcher runtime | 10 / 72 assertions | Actual complete source |
| Hexing strict audit | 11 fresh proofs | Zero cached cards |
| Historical strict prefix | 145 cards / 395 fresh proofs | Zero cached cards |
| Runtime CI3 | 363 | Reused source837 evidence, no new CI3 run |
| Architecture | 0 violations / 0 warnings | Fresh |

Runtime Rust is unchanged from source837. The certificate pins the inherited CI3
log and unchanged runtime inputs; this parser-only lot does not claim another
engine compilation or CI3 execution. Exact Hexing linking checks all four emitted
rules and full metadata for all eleven physical source copies in ten scenarios.
The passive linker handles opposing source definitions explicitly; source.rules
is never replaced by an isolated ability. These scenarios cover intrinsic/granted
pay and decline, insufficient life, pending Ward after departure, grant removal,
actual controller change, opponent exclusion and same-name source identity.

The ledger is now 175 cards: only Hexing Squelcher was appended, and all prior
174 rows remain exact. Parser-wide gains are not individual behavior certification.

## Coverage and timings

The unchanged 39,288-card input now has 3,916 complete nonempty parses and 18,027
native abilities: +16 complete cards and +63 abilities. All 17,964 prior native
rules are case- and JSON-type-exact, with zero allowed rewrites or regressions.
Partitions: 3,916 complete + 11,079 partial + 21,148 none + 3,144 empty canonical
+ 1 empty unsupported = 39,288.

The same ten frozen sets contain 2,368 occurrences: 311 complete + 924 partial
+ 1,130 none + 3 empty; 1,592 native abilities. Gains are +3 complete occurrences
and +7 abilities. Deduplication is separate: 2,319 oracle IDs = 263 complete
+ 923 partial + 1,130 none + 3 empty. Cohort11 has 2 complete cards and 36/209
native abilities; Hexing adds one complete card and two abilities. No refill or
new cohort is adopted by this report.

Measured parser focused run: compile 15.018 s, execution 0.060 s. Broad parser
units: compile 18.072 s, execution 1.600 s; cache maintenance 2.130 s separately.
Hexing's ten linked runtime scenarios execute in 0.307 s with zero compilation.
Release analyzer build is 31.259 s plus 1.299 s link. Thirteen immutable-input
audits execute in 11.539 s (global 10.509 s); strict typed comparison takes
18.879 s. These are different stages, not a simulation speedup claim.

Root timing records 938.919 s elapsed, including concurrent ordered-kernel work:
82.421 s compilation, 47.119 s execution and 774.766 s outside measured commands.
That remainder includes provenance scripts, certification and concurrent kernel
work; it is not Hexing implementation time. The shared feature unlocks sixteen
complete parses. Dividing elapsed time by one certified card would misrepresent
the work. Manual proof orchestration is the workflow automation target.

## Limits and next work

The known compound-cost mana bug remains unresolved: source837/source844 may
credit colors projected before sacrificing a colored source. Exact Bloom Tender
tap-only proofs remain valid. The certificate carries the unresolved post-cost
erratum; the ordered post-cost evaluation/LKI correction is staged separately.
Do not treat this Ward acceptance as certification of that neighboring family.

Static ColorSet P/T remains held. All generic primitives are retained regardless
of parser emission. The 22 specialized retirements remain enforced; the four
native exile compositions still require replacement proofs. The ledger, raw
source inputs, adoption order and unrelated engine/training work are preserved.

Evidence is pinned by certificate.evidence, including typed-comparison.json,
all gate logs/reports, strict audits, actual semantic pins, source-link manifest
and exact-source-proof.json. Comparison SHA256:
`D9B455DCB963A1473391AD0B1E07B393CCF38A1FBE9C1F83226AF54C32D8B8E3`.
