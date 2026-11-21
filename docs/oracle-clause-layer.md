# Couche de proposition : un verbe, une entrée, tous les cadres (21 novembre 2026)

## Diagnostic

Le contrat (`oracle-compositional-compiler.md`) demande que le lexique fournisse
les formes et que la grammaire décide le rôle. Le code partage bien les
**opérandes** (requête d'objets, expression numérique, durée, atomes de
condition) mais pas les **prédicats** : il n'existe aucune couche « sujet +
verbe + arguments ». Chaque cadre parent reconnaît ses verbes par des mots
littéraux et reconstruit leurs arguments.

Mesuré sur la chaîne fusionnée (#26 + #27…#45), 442 fichiers d'analyseur :

| Symptôme | Mesure |
|---|---|
| Mot de verbe testé à la main | `gains` 16 fichiers, `destroy` 14, `becomes` 11, `deals` 10, `loses` 10, `draws` 8, `gets` 8 |
| Énumérations de formes verbales | 5 (`LifeVerbForm`, `DiscardVerbForm`, `PayLifeVerbForm`, `SubjectVerbAgreement`, `TypeChangeNumber`) et 6 conjugaisons ad hoc (`format!("{verb}s")`, `third_person_form`, booléens) |
| « X gain(s)/lose(s) N life » | 6 chemins (effet, action de joueur, joueurs quantifiés, événement, deux conditions, remplacement) |
| « X sacrifice(s) … » | 2 lecteurs d'événement distincts, chacun avec son accord, plus effet, coût et sacrifice ordinaire |
| « SRC deal(s) N damage » | effet, 4 lecteurs d'événement, remplacement, condition ; deux énumérations identiques `DamageEventKind` / `DamageKindFilter` |
| « X pays COST » | `consume("pays")` direct dans 4 fichiers |
| Nœuds IR | un même lemme produit 3 à 6 nœuds différents selon le parent (ex. vie : `GainLife`, `LoseLife`, `TargetPlayerLife`, `PlayerAction::Life`, `QuantifiedPlayerAction::Life`, `PlayerEventAction::GainLife`) |

Conséquence directe, observée à la fusion des 19 thèmes : « without KEYWORD »
écrit 3 fois, « becomes blocked » 2 fois (deux noms d'événement), le contrôle
d'Aura 2 fois, la clause de combat 2 fois, le compteur « for each » 2 fois.
Le coût d'un verbe est `rôles × formes × sujets` au lieu de 1. Chaque thème
ne débloque que la case (verbe, rôle) qu'il touche : c'est la lenteur.

## Cible

```text
Clause      := Sujet? Polarité? Modalité? Verbe[accord] Arguments
Sujet       := impératif | joueur | ensemble de joueurs | objet | ensemble d'objets
               (lecteur de sujet partagé, nombre grammatical inclus)
Verbe       := lemme du lexique, toutes ses formes (base, 3sg, passé, participe, -ing)
Arguments   := cadre d'arguments déclaré par le lemme : emplacements typés
               (requête d'objets, quantité, joueur, zone, mots-clés, P/T, durée)
```

1. **Lexique des verbes (données)** : `lemme → { formes, cadre d'arguments,
   voix admises (actif, passif « is dealt »), constructeur sémantique }`. Une
   seule source pour la conjugaison et l'accord ; les 5 énumérations et les 6
   conjugaisons ad hoc disparaissent.
2. **Lecteur de proposition unique** : `recognize_clause(cursor, ClauseFrame)`.
   Le cadre décrit ce que le parent attend, pas les verbes :
   - politique de sujet : impératif, explicite, hérité, causatif (`have X VERB`) ;
   - accord : déduit du sujet (nombre), forme nue après causatif ;
   - polarité et modalité : affirmatif, `can't`, `doesn't`, `would`, `may` ;
   - temps : présent ponctuel, passé (`this turn`), passif.
   Il rend `Clause { sujet, action: ActionNode, polarité, temps, voix }` et
   reste transactionnel (le sujet n'engage rien tant que le verbe n'est pas pris).
3. **Un nœud d'action par lemme** (`ActionNode::DealDamage { source, amount,
   recipients }`). Les rôles deviennent des **interprétations** de la même
   proposition :

   | Parent | Lecture de la proposition | Sortie |
   |---|---|---|
   | séquence d'effets | présent affirmatif | action ponctuelle |
   | déclencheur `whenever` | même action, emplacements lus comme filtres | motif d'événement |
   | remplacement `if … would` | même motif + modification | remplacement |
   | condition `if you VERBed this turn` | passé | requête d'historique sur le même motif |
   | statique | continu (`get`, `have`, `can't`) | modificateur continu |
   | coût | impératif en position de coût | paiement |

   Un motif d'événement est l'action dont les opérandes sont des prédicats :
   le moteur compare chaque événement émis à ce motif par une seule fonction,
   au lieu d'un `kind` d'événement par phrase (`permanentBecameBlocked`,
   `controlledCreatureBecameBlocked`, `damageDealtBySource`…).

## Ce que cela débloque sans code nouveau

Chaque lemme ajouté au lexique devient disponible dans tous les rôles que son
moteur sait exécuter : `whenever a player sacrifices / discards / mills /
searches …`, `if you sacrificed … this turn`, `have target player VERB`,
`each opponent VERBs`, `VERB … : cost`. Aujourd'hui chacune de ces cases est un
thème.

## Migration (par lemme, sans régression)

1. Lexique + `recognize_clause` + `ActionNode` pour un premier lemme très
   dupliqué (**vie**), branchés dans les 6 parents ; les anciens lecteurs du
   lemme sont supprimés dans la même PR. Preuve : égalité structurelle de l'IR
   sur les fixtures existantes, sondes ligue/corpus sans carte perdue, une
   combinaison inédite (ex. `whenever an opponent loses life` + causatif).
2. Puis, dans l'ordre de duplication : dégâts, sacrifice, défausse, pioche,
   gain de mot-clé / `get`, `becomes`, attaque / blocage, création de jetons,
   paiement, destruction, exil, retour, meule, recherche, engager / dégager.
3. Côté moteur : un correspondant de motif d'événement générique ; les `kind`
   d'événement par phrase sont réécrits en motifs au fil des lemmes.
4. Indicateurs suivis à chaque PR : nombre de sites `eq_ignore_ascii_case("<verbe>")`
   hors lexique (cible 0), nombre d'énumérations de formes (cible 1), cases
   (lemme × rôle) exécutables, cartes supportées.

## Règles pour les agents pendant la migration

- Aucun nouveau lecteur de verbe hors lexique ; un verbe manquant s'ajoute au
  lexique avec son cadre d'arguments.
- Un parent ne teste jamais un mot de verbe : il appelle `recognize_clause` avec
  son `ClauseFrame` et interprète le `Clause` rendu.
- Les thèmes de couverture (C2–C10 du plan) reprennent ensuite **par lemme
  entier** (tous les rôles d'un coup), pas par case.

## Phase 1 réalisée : lemme vie (`gain` / `lose`)

- `automata/verb_lexicon.rs` : `VerbLemma`, `VerbForm` (base, 3sg, passé,
  participe, -ing), `VerbAgreement` (seul mécanisme d'accord ; remplace
  `LifeVerbForm`), `ArgumentFrame::LifeTotalChange`, voix.
- `automata/clause.rs` : `recognize_clause(cursor, &ClauseFrame)` →
  `ClauseRecognition::{Recognized, NotClaimed, Failed}`. Politiques de sujet
  `EffectPlayer` (impératif, explicite, hérité, causatif), `PlayerScope`
  (`you`, `a player`, `an opponent`, `one or more`) et `Supplied` (ensembles
  de joueurs, sujets coordonnés, acteur d'événement) ; modalité `would` ;
  temps présent ou passé `this turn` (passé simple ou parfait `have`/`you've`) ;
  emplacement de quantité `Expression | EventQuantity | Absent | Threshold |
  Gapped`. Le sujet reste provisoire tant que le verbe et le début de son cadre
  d'arguments (`life`) ne sont pas pris.
- Interprétations : séquence d'effets (`life_clause.rs`), éléments des listes
  quantifiées et coordonnées, événement `whenever`, atome d'historique
  (`static_condition.rs`), fait numérique relatif (`the amount of life you
  gained this turn`), remplacement `if … would gain life`.
- IR inchangée ; les nœuds parents (`GainLife`, `TargetPlayerLife`,
  `PlayerAction(Life)`, `QuantifiedPlayerAction(Life)`) restent les
  abaissements, choisis par le genre de sujet de la proposition.
- Restent hors lexique : `PayLifeVerbForm`, `DiscardVerbForm`,
  `SubjectVerbAgreement`, `TypeChangeNumber` et les sens « mot-clé » de
  `gain`/`lose` (`gains flying`, `loses all abilities`).
