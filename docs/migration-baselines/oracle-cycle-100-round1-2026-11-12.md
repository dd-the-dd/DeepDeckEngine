# Cycle Oracle — lot 100, ronde 1

Ce rapport sépare deux cohortes qui ne doivent pas être additionnées : le lot rotatif figé de 100 cartes, où se trouvent les cinq cartes ciblées, et les 2 069 cartes des sept sets locaux, qui ne contiennent aucune de ces cinq cartes.

Une carte est **native complète** seulement si elle est canonique et si toutes ses capacités sont canoniques via le chemin token/FSM natif. `mixed` combine capacités natives et legacy; `legacy` n'en contient aucune native; `unsupported` contient au moins une capacité non reconnue.

## Lot rotatif figé de 100

| Classe | E9B08 avant | F7DC7B8 après | Delta |
|---|---:|---:|---:|
| Native complète | 0 | 5 | +5 |
| Mixte | 23 | 21 | -2 |
| Legacy | 55 | 53 | -2 |
| Unsupported | 22 | 21 | -1 |

Les cinq gains complets sont :

| Carte | Avant | Généralisation |
|---|---|---|
| Demonic Tutor | Unsupported | `librarySearchMove` |
| Enlightened Tutor | Legacy | `librarySearchMove` |
| Vampiric Tutor | Legacy | `librarySearchMove` |
| Thought Vessel | Mixte | `noMaximumHandSize` |
| Reliquary Tower | Mixte | `noMaximumHandSize` |

Il n'y a aucune régression de couverture ni changement sémantique parmi les cartes précédemment natives. L'audit debug a pris 7,159 s avant et 6,862 s après (13,968 contre 14,573 cartes/s). Ce delta de -4,15 % est une variance observée, pas une preuve d'accélération.

Les cinq cartes disposent du chemin complet demandé : 61/61 cas parser verts, égalité octet-canonique parser/runtime pour les trois Tutors et les deux cartes de taille de main, puis 9/9 fixtures runtime et 68/68 assertions. Les quatre tests unitaires ciblés du mélange de bibliothèque sont également verts.

## Sept sets locaux — même cycle E9B08 → F7DC7B8

Les cinq cartes ci-dessus sont hors de ce corpus. Il est donc attendu que le total de cartes natives complètes reste à 133.

| Classe | Avant | Après | Delta |
|---|---:|---:|---:|
| Native complète | 133 | 133 | 0 |
| Mixte | 402 | 403 | +1 |
| Legacy | 594 | 594 | 0 |
| Unsupported | 940 | 939 | -1 |

La seule transition est **Obstinate Gargoyle** (`mh3`) : `unsupported → mixed`. Persist est maintenant natif et canonique; sa capacité « flying as long as modified » demeure legacy.

| Set | Cartes | Native | Mixte | Legacy | Unsupported |
|---|---:|---:|---:|---:|---:|
| DSK | 264 | 11 | 37 | 41 | 175 |
| ECL | 262 | 7 | 75 | 155 | 25 |
| EOE | 254 | 7 | 37 | 72 | 138 |
| FDN | 422 | 49 | 86 | 97 | 190 |
| FIN | 306 | 19 | 44 | 44 | 199 |
| MH3 | 281 | 17 | 40 | 43 | 181 |
| TLA | 280 | 23 | 84 | 142 | 31 |

Les 133 cartes précédemment natives ont zéro recul de couverture et zéro changement d'IR sémantique. L'audit debug a pris 289,850 s avant et 275,624 s après (7,138 contre 7,507 cartes/s); le delta de -4,91 % reste une variance debug, pas une mesure de vitesse concluante.

## Périmètre historique pré-reminder/P-T

La comparaison mécanique du même corpus et du même classificateur donne `112 → 133`, donc **21 nouvelles cartes classées natives par reconnaissance**, sans perte. Ce compteur historique est distinct des preuves du parcours complet des cinq cartes du lot : le manifeste de preuves et le préfixe historique n'ont pas été recertifiés pendant ce cycle. Il ne faut pas présenter `106 → 133` comme un gain de 27 : l'ancien rapport à 106 excluait six cartes vanilla à cause d'un rejet de contrat faussement négatif. Les six cartes vanilla font partie du total actuel de 133, pas du gain mesuré de 21.

## Ronde 2

Le nouveau lot conserve exactement 95 cartes non couvertes et ajoute les cinq suivantes selon leur provenance rotative : Faeburrow Elder, Morophon, the Boundless, Big Score, Ghostly Prison et Deafening Silence. Un audit frais confirme `0 native / 22 mixtes / 56 legacy / 22 unsupported`.

Le meilleur correctif suivant est le coût alternatif conditionnel « If you control a commander » : **Fierce Guardianship** et **Deadly Rollick** seraient deux gains complets. Deflecting Swat partage ce coût, mais son corps demeure un blocage indépendant. Viennent ensuite `additional land play` (Exploration, un gain complet) et `source enters → draw` (Baleful Strix, un gain complet).

## Artefacts

- Rapport machine détaillé : `docs/migration-baselines/oracle-cycle-100-round1-2026-11-12.json`
- Comparaison lot 100 : `.tmp/rotating-next-100-comparison-E9B08-to-F7DC7B8.json`
- Comparaison sept sets : `.tmp/seven-local-sets-comparison-E9B08-to-F7DC7B8.json`
- Profil de validation 61 cas : `.tmp/native-profile-batch100-final.json`
- Preuves runtime : `.tmp/batch100-runtime-reviewed.jsonl` — 9 fixtures, 68 assertions
- Égalité IR Tutors : `.tmp/library-search-parser-runtime-equality.json`
- Égalité IR taille de main : `.tmp/no-maximum-hand-size-ir-equality.json`
- Tests ciblés de mélange : `.tmp/library-shuffle-focused-unit-timing.json`
- Lot ronde 2 : `.tmp/rotating-next-100-round2.json`

## Temps de validation

| Phase | Compilation / liaison | Exécution |
|---|---:|---:|
| Build parser + runner | 30,993 s | 0,388 s à chaud pour 61 cas (`1,862 s` avec l'enveloppe de profil) |
| Build runtime | 52,239 s | pas de chronométrage isolé enregistré pour ce passage de neuf fixtures |
| Tests unitaires ciblés library shuffle | 1,209 s | 0,225 s pour 4 tests |

Les gardes couvrent 908 tests répartis en partitions, 54 fichiers de tests moteur dédiés, zéro violation d'architecture et quatre avertissements historiques. Un premier essai du wrapper parser a été interrompu parce que stderr de Cargo était interprété comme une erreur; `scripts/run-parser-fixture.ps1` a été corrigé avant la mesure réussie.
