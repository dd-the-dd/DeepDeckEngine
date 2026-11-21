# Couverture native des 10 derniers sets — 2026-11-12

## Résultat

Cette mesure utilise exclusivement le parseur token/FSM en mode `NativeOnly` du checkpoint B13. Elle mesure la **reconnaissance native canonique du parseur**; elle ne constitue pas une certification runtime individuelle de chaque carte.

| Set | Sortie | Oracle IDs | Complètes avec capacité | Vanilles | Partielles | Aucune capacité native | Capacités natives / total | Temps analyseur |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| MBC — Mystery Booster Commander Edition | 2026-10-19 | 80 | 8 | 0 | 15 | 57 | 26 / 181 (14,36 %) | 0,141 s |
| FRA — Reality Fracture | 2026-10-02 | 285 | 33 | 2 | 108 | 142 | 176 / 562 (31,32 %) | 0,365 s |
| HOB — The Hobbit | 2026-08-14 | 193 | 10 | 1 | 79 | 103 | 100 / 387 (25,84 %) | 0,199 s |
| MSH — Marvel Super Heroes | 2026-06-26 | 281 | 16 | 0 | 140 | 125 | 192 / 590 (32,54 %) | 0,287 s |
| SOS — Secrets of Strixhaven | 2026-04-24 | 271 | 31 | 0 | 99 | 141 | 172 / 506 (33,99 %) | 0,289 s |
| TMT — Teenage Mutant Ninja Turtles | 2026-03-06 | 195 | 14 | 0 | 78 | 103 | 105 / 408 (25,74 %) | 0,202 s |
| ECL — Lorwyn Eclipsed | 2026-01-23 | 273 | 20 | 0 | 80 | 173 | 120 / 540 (22,22 %) | 0,313 s |
| TLA — Avatar: The Last Airbender | 2025-11-21 | 286 | 30 | 0 | 94 | 162 | 169 / 604 (27,98 %) | 0,510 s |
| SPM — Marvel's Spider-Man | 2025-09-26 | 193 | 13 | 0 | 86 | 94 | 113 / 400 (28,25 %) | 0,228 s |
| EOE — Edge of Eternities | 2025-08-01 | 266 | 19 | 0 | 81 | 166 | 122 / 519 (23,51 %) | 0,458 s |
| **Occurrences par set** | — | **2 323** | **194** | **3** | **860** | **1 266** | **1 295 / 4 697 (27,57 %)** | **2,992 s** |

Les sommes par set comptent un même `oracle_id` une fois dans chaque set où il apparaît. Après déduplication entre les dix sets, l'union contient **2 279 Oracle IDs** : 151 cartes complètes avec capacité, 3 vanilles, 859 partielles et 1 266 sans capacité native canonique. Cela donne **154/2 279 cartes complètes (6,76 %)** et **1 251/4 652 capacités natives (26,89 %)**.

Le temps mur cumulé des dix processus est de 10,206 s; le temps rapporté par l'analyseur est de 2,992 s. La lecture initiale du catalogue local a pris 2,044 s. Aucun Cargo n'a été lancé.

## Définitions

- **Complète avec capacité** : `row.status == canonical`, au moins une capacité, et toutes les capacités ont `status == canonical` et `recognizer == native`.
- **Vanille** : `row.status == canonical`, aucune capacité émise, et aucun texte Oracle non vide au niveau carte ou face. Aucun échec de découpage vide n'a été observé.
- **Partielle** : au moins une capacité native canonique, mais la carte ne satisfait pas la définition complète.
- **Aucune capacité native** : zéro capacité native canonique; cela inclut les échecs structurés au niveau carte.

Les capacités des cartes multifaces sont aplaties dans une ligne par `oracle_id`. Les objets complets, leurs faces et leur provenance d'impression sont conservés dans les entrées dédupliquées. La vérification indépendante confirme **99 cartes multifaces** préservées.

## Population et provenance

Les dix sets sont les plus récents, à la date du 2026-11-12, dont le type Scryfall est `expansion`, `core`, `masters` ou `draft_innovation`, avec `digital == false` et `released_at <= 2026-11-12`. MBC est donc inclus comme `draft_innovation`; FIN serait le onzième set et n'entre pas dans ce total.

- Snapshot cartes Scryfall `default_cards`, mis à jour le 2026-11-09T10:06:40.990Z : SHA-256 `56EF03C42D72E906EE6B683D9C12E14A195019FE894F24FCEE8A8368C220E770`.
- Index sets local du 2026-11-12 : SHA-256 `6ACD357205353F19130548087F70B8681115A2A80D8EA311E4726A086485B81D`.
- Validation indépendante de l'extraction : SHA-256 `ADBABA8BF02A4C7A18E5C5BB1AE4578AD86CF2F4369FEA2D4E7573ED7088ADB2`, `allExact == true`.
- Analyseur B13 immuable : SHA-256 `3E4A72043EA8088E6F1F9386AB4B19A9A617C197EC1382C734F6958941DD8DC1`.
- Rapport détaillé temporaire corrigé : SHA-256 `6151865E7034BFB098DC44C1131F238FA3B70D45C17F69930C01DF9E17C55818`.
- Archive exacte antérieure à la normalisation des faces nulles : SHA-256 `CAD7DB5C279EA5DAACC26C89822FC85EF80ADB2D56C343EFF39F65F2CB613EBC`.
- Vérification indépendante des résultats et des sommes dérivées : SHA-256 `1C847AD6EDD359A2214DDA48B685BC30C488E00412A2C43E2D7694A5937D6F98`, `passed == true`.

Les cinq wrappers ECL sans `oracle_id` ont été exclus du dénominateur. Leurs faces correspondent à cinq objets top-level déjà présents; aucune carte Oracle n'est perdue.

## Artefacts

- Résumé durable : `docs/migration-baselines/last-10-sets-native-coverage-2026-11-12.json`
- Tableau durable : `docs/migration-baselines/last-10-sets-native-coverage-2026-11-12.csv`
- Rapport détaillé avec cartes, faces et provenance : `.tmp/recent-sets-native-B13/last-10-sets-nativeonly-B13B4687.json`
- Vérification indépendante : `.tmp/recent-sets-native-B13/root-independent-verification.json`

