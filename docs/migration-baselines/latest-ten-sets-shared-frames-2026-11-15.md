# Dix dernières extensions — cadres partagés, 15 novembre 2026

Branche `oracle/verb-frames-generalization`, non commitée. Mesure de parsing natif
seulement (`DEEPDECK_ORACLE_NATIVE_ONLY=1`) : une carte « complète » a toutes ses
capacités en IR native ; ce n'est pas une preuve d'exécution par carte.

Reproduire : `python scripts/audit-latest-sets.py --label after --compare-to before`
(sélection : 10 dernières extensions non numériques de l'index Scryfall en cache,
un représentant par `oracle_id`, données locales `.tmp/default-cards.jsonl.gz`).

| Set | Cartes complètes | Capacités natives |
|---|---:|---:|
| Star Trek | 47 → **50** / 311 | 212 → **224** / 586 |
| Reality Fracture | 55 → **56** / 285 | 235 → **242** / 562 |
| The Hobbit | 23 → **30** / 193 | 135 → **153** / 387 |
| Marvel Super Heroes | 32 → **34** / 281 | 228 → **249** / 590 |
| Secrets of Strixhaven | 64 → **66** / 271 | 243 → **246** / 506 |
| Teenage Mutant Ninja Turtles | 43 → **45** / 195 | 158 → **171** / 408 |
| Lorwyn Eclipsed | 42 → **43** / 273 | 164 → **173** / 540 |
| Avatar: The Last Airbender | 50 → **52** / 286 | 227 → **241** / 604 |
| Marvel's Spider-Man | 28 → **29** / 193 | 144 → **157** / 400 |
| Edge of Eternities | 29 → **32** / 266 | 154 → **173** / 519 |
| **Total (occurrences)** | **413 → 437** / 2554 (16,2 → 17,1 %) | **1900 → 2029** / 5102 (37,2 → 39,8 %) |

Union dédupliquée : 356 → 380 cartes complètes sur 2495 `oracle_id`.
Comparaison par capacité : **+129 gagnées, 0 régression, 0 IR modifiée**.

## Changements (un concept, pas une phrase)

| Étape | Concept | Gain capacités |
|---|---|---:|
| 1 | `this <sous-type>` désigne la source (CR 205.3, snapshot officiel déjà présent) ; cadre verbal `tap OBJET` sur l'opérande permanent partagé | +30 |
| 2 | Mot de saveur `Label —` sans lexème du catalogue = étiquette sans règle (CR 207.2c) | +2 |
| 3 | Cadre statique continu `Sujet (get(s) ±a/±b \| have/has mots-clés) (and …)* .` sur source, équipé/enchanté ou collection | +64 |
| 4 | `Activate only CLAUSE (and only CLAUSE)* .` lu une fois, appliqué à la capacité (`activationCondition`, `activationLimit`) | +29 |
| 5 | `for each COLLECTION` comme multiplicateur du delta statique (`countPermanents`) | +4 |

Le moteur exécutait déjà `tapPermanent`, `modifyPowerToughness`/`grantKeyword`
statiques sur `self`/`attachedPermanent`/`permanents`, `sorceryTiming`,
`duringControllerTurn` et `oncePerTurn` : aucune nouvelle opération moteur.

## Tests

`cargo test --release -p deepdeck-oracle --lib` : 1064 réussis, 5 échecs
**préexistants** (comportement identique entre le binaire avant et après sur les
mêmes phrases) : `compact_mana_envelope_reuses_sequence_connectors_for_followups`,
`delegates_death_event_and_independent_life_actions_to_shared_children`,
`imperative_life_action_defaults_to_controller_but_third_person_does_not`,
`default_preserves_native_phase_failure_without_fallback`,
`third_person_life_verb_is_never_defaulted_to_controller`.
Deux tests figeant l'ancienne limite « Activate only » ont été mis à jour ;
`activated_combat_restriction` refuse désormais une séquence restreinte au lieu
d'ignorer la restriction. Aucune preuve runtime nouvelle n'a été ajoutée.

## Prochains concepts mesurés (cartes débloquées seules)

`If you do` / `When you do` après une action facultative (≈44), triggers modaux
(27), `where X is EXPR` postposé (≈26), `Power-up —` (18), `until end of turn, …`
antéposé (15), `as long as CONDITION` statique (35 capacités), sous-type singulier
dans une collection (`each other Goblin you control`).
