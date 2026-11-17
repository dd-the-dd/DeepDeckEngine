# Cycle Oracle - lot 100, ronde 2

Cette ronde generalise la condition de cout alternatif `If you control a commander`. Le chemin natif produit une capacite `alternativeCost` conditionnelle et compose `countPermanents(controller, where isCommander) >= 1`; aucun chemin nomme par carte ou operation composite n'est conserve.

## Delta du lot rotatif de 100

| Classe | Avant | Apres | Delta |
|---|---:|---:|---:|
| Native complete | 0 | 2 | +2 |
| Mixte | 22 | 21 | -1 |
| Legacy | 56 | 55 | -1 |
| Unsupported | 22 | 22 | 0 |

Transitions :

- Fierce Guardianship : `mixed -> native`.
- Deadly Rollick : `mixed -> native`.
- Deflecting Swat : `legacy -> mixed`; la condition de cout est native, mais le changement de cible reste legacy.

Le prefixe historique de certification reste inchange; cette ronde ne le recertifie pas et ne doit pas etre presentee comme une extension de ce prefixe.

## Preuves

- Parseur data-driven : 65/65 cas verts. Execution chaude mesuree a 0,273 s; profil complet dans `.tmp/native-profile-commander-reviewed.json`.
- Egalite IR parseur/runtime : `.tmp/commander-alternative-cost-ir-equality.json`.
- Runtime apres suppression du fallback legacy : `.tmp/commander-runtime-without-legacy.jsonl`, 15/15 fixtures et 89/89 assertions. Temps dans `.tmp/commander-runtime-without-legacy-timing.json`.
- Builds runtime : 34,10 s pour le premier build, puis 41,874 s apres suppression de `freeIfControlCommander`.
- Architecture : 909 tests repartis dans 55 fichiers, zero violation.
- Regression corpus local 2 069 cartes : classes stables. Les 1 109 capacites deja natives ont zero perte et zero changement semantique dans `.tmp/native-ability-regressions-two-cycles.json`.
- Deux contrats data-driven, Fierce Guardianship et Deadly Rollick, sont presents mais leur runner doit encore etre rafraichi apres le benchmark. Le runtime prouve deja leur executabilite complete.

Le fixture exact Deadly Rollick donne maintenant quatre compteurs `loyalty` au Planeswalker commandant de test afin que la position initiale soit legale. Sa relance directe reste verte : 5/5 assertions. Ce raffinement est posterieur au rapport runtime principal et n'en change pas la semantique testee.

## Semantique moteur prouvee

- Seul un permanent designe `isCommander` sur le champ de bataille compte.
- Une carte Legendary ordinaire ne compte pas.
- Un commandant en command zone ne compte pas.
- Un commandant adverse controle par le lanceur compte.
- Le cout alternatif vide remplace le cout de mana de base sans supprimer les couts additionnels.
- Les effets et cibles des deux cartes exactes restent executes par les primitives ordinaires `counterSpell` et `exilePermanent`.

## Suivi de qualite 107.3b

Le gap `{X}` detecte pendant cette ronde a ete corrige generiquement apres la promotion des deux cartes. Le runtime final prouve X0 pour un cout gratuit, X3 pour le cout normal, le paiement reel d'une alternative `{X}{B}` et le paiement d'un cout additionnel `{X}`. Les details et la limite explicite des couts non-mana sont dans `docs/migration-baselines/known-gaps/free-alternative-cost-x-107-3b.md`. Cette correction de qualite n'ajoute aucune carte au delta de couverture de la ronde.
