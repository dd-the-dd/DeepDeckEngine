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

## Phase 2 réalisée : cadres multiples et lemme dégâts (`deal`)

- Lexique : une entrée porte un **ensemble** de cadres (`gain` : vie, liste de
  mots-clés, contrôle ; `lose` : vie, capacités ; `deal` : dégâts) et ses voix
  (`deal` : actif et passif). Le parent déclare les cadres qu'il interprète
  (`ClauseFrame::arguments`) ; le lecteur choisit, dans l'ordre du lexique, le
  premier cadre accepté dont les arguments commencent après le verbe. Un cadre
  déclaré sans lecteur (`ClauseArguments::Unread`) est revendiqué puis échoue
  explicitement au lieu d'être pris pour un autre.
- Voix passive : auxiliaire `is`/`are` accordé au sujet, `be` après `would`,
  puis participe passé. Sujet « dégâts » (`[combat] damage would be dealt to
  R`) : politique `DamageTheme`.
- `damage_clause.rs` : politiques du cadre `DamageDealt` (effet, complément
  d'événement, passif, événement et destinataires de remplacement, quantité
  d'événement, montant atomique, relative à objet extrait) appelant les
  enfants existants. Un seul `DamageKind` remplace `DamageEventKind` et
  `DamageKindFilter`.
- Parents migrés : effet (`recognize_damage_predicate`, causatif, revendication
  de `permanent_action`), déclencheurs (`trigger_event` source/attaché,
  `quantified_event`, `player_event`), remplacement (`damage_replacement`
  actif, passif et modification), condition (`conditional.rs`), prévention
  (relatives `would deal` / `that would be dealt`). IR inchangée.
- Suite (moteur) : les sélecteurs de source `objects` (remplacement, sorts
  admis) et `permanents` (observation de déclencheur, champ de bataille) ont
  une sémantique différente ; les unifier changerait le JSON émis.

## Phase 3 réalisée : propagation des rôles (coût, événements)

- Lexique : `sacrifice`, `discard`, `tap`, `untap`, `exile`, `return`, `put`,
  `reveal`, `mill`, `exert`, `remove`, `pay`, `blight`, `create`, `cycle`, avec
  leurs cadres (`ObjectSelection`, `ObjectToZone`, `CountersOn`,
  `CountersFrom`, `CardCount`, `ResourcePayment`, `KeywordAmount`,
  `TokenCreation`). `ActingSubject` (accord + chooser tiers) remplace
  `DiscardVerbForm` ; `ImperativeActionVerb`, `PayLifeVerbForm`, la liste
  `PAYMENT_VERBS` et la conjugaison `{verb}s` sont supprimés.
- `ClauseArguments::Operand(frame)` / `recognize_operand_verb` : la
  proposition lit sujet, lemme, forme et cadre ; le parent lit l'opérande avec
  son enfant de rôle.
- Le cadre de coût est client de la proposition : tout lemme du lexique dont
  un cadre est lisible comme coût y est disponible. Abaissement d'activation
  branché sur les primitives moteur existantes (`returnPermanentToOwnersHand`
  choisi ou source, `putCountersOnSource`, `putCounters` choisi) ; `untap`,
  `reveal`, `mill`, `exert` échouent explicitement (pas de schéma de coût).
- Événements : `sacrifice_event`, `actor_object_event` et les verbes d'objet
  de `player_event` lisent le lemme par la proposition.
- Reste : le cadre de permission (`you may VERB … rather than pay`, `exert …
  as it attacks`, `reveal … from your opening hand`) dépend de mécaniques
  moteur absentes ; les lecteurs d'effet de ces lemmes gardent leurs tests de
  mots ; `remove` comme effet n'a pas de primitive moteur.

## Phase 4 réalisée : rôle effet par la proposition

- `lemma_at(cursor, lemme, accord)` et `consume_lemma(cursor, lemme, accord)`
  (`clause.rs`) : le dispatch et les positions de verbe fixes des lecteurs
  d'effet lisent le lemme par la proposition partagée (tous ses cadres, le
  parent gardant l'opérande). Les tests de mots restants des portes de famille
  (fenêtres de jetons) interrogent le lexique (`is_verb_form`).
- `eq_ignore_ascii_case("<verbe>")` hors lexique pour sacrifice, discard,
  tap, untap, exile, return, put, reveal, mill, create, gain, lose, pay :
  120 → 0. Les lecteurs d'effet de joueur (`player_action`, listes
  quantifiées) dispatchent par lemme.
- Reste : tables de phrases (`consume_phrase(&["loses", "all", ...])`,
  rappels de mots-clés), clés `match` de quelques lecteurs de famille ;
  `conjure` non ajouté (il faut un nœud d'effet, un lecteur de nom et une
  preuve runtime dépendante du catalogue, pas seulement une entrée de lexique).

## Phase 5 (partielle) : motifs d'événement de la source, S5/S2

- Lexique : `attack`, `block`, `become`, `die`, `enter`, `leave`, `cast`,
  `draw` (cadres `Intransitive`, `StateChange`, `ZoneDeparture`,
  `SpellObject`, `CardCount`).
- Le lecteur d'événement de la source lit ses verbes par la proposition et
  porte des modificateurs typés (`SourceEventModifier`) : contrepartie
  (`blocks a creature` / `becomes blocked by a creature` est le même événement ;
  un qualificatif est un manque moteur), `alone`, `and isn't blocked`,
  `for the first time each turn`. Sans correspondant moteur, l'abaissement
  échoue en `trigger.event.engine-gap`, distinct de la grammaire.
- S5/S2 : `each player|opponent|other player` n'est jamais lu comme
  quantificateur d'objets ; si le lecteur d'ensemble de joueurs ne prend pas le
  verbe, l'échec est `clause.verb.unsupported` (lemme et genre de sujet).
- Reste : un lecteur d'événement unique pour tous les sujets (quantifiés,
  joueurs, attachés, sorts), les correspondants moteur des modificateurs, et
  les lecteurs par verbe de `trigger_event.rs`, `quantified_event.rs`,
  `player_event.rs`, `spell_cast_event.rs`, `targeting_event.rs`.

## Phase 6 (en cours) : politique de sujet d'effet

Mesure sur la chaîne après #54 : 3 149 cartes échouent au moins une fois en
`effect.start` « expected a supported effect subject », dont 1 986 n'ont que
ce bloqueur. Une phrase d'effet est `Sujet + Proposition` ; le sujet est
l'impératif ou `you`, une référence du résolveur de liaisons, un ensemble
quantifié (joueurs, objets, cadre distributif `for each X, …`) ou un groupe
nominal défini lié au cadre. La proposition prend son accord du sujet.

### Étape 13 : `you` sujet explicite

- `you VERB …` est la proposition de l'impératif accomplie par le
  contrôleur (CR 109.5) : quand aucun lecteur ne prend la phrase, la séquence
  consomme `you` (`effect.subject.controller`) et relit la même proposition
  avec le contrôleur comme sujet hérité. Un sujet explicite remplace celui
  qu'une proposition précédente prêterait (`target player draws …, then you
  mill …`). `controller_subject_starts` (`clause.rs`) exclut les suites
  modales, négatives et relatives (`you may`, `you don't`, `you control`).
- Un verbe qu'aucun lecteur ne prend échoue en `clause.verb.unsupported`
  (lemme ou mot, sujet « the controller (`you`) ») au lieu de
  `effect.start`.
- Sonde : ligue 809 → 809, corpus 14319 → 14329 (+10), 0 perte, IR des
  cartes supportées inchangée. Preuve : IR identique entre `you VERB` et
  l'impératif (cartes réelles et recombinaisons inédites), sujet hérité
  remplacé, quasi-échec `you get an emblem`.

### Étape 14 : lemme `get`, cadre compteurs de joueur

- Lexique : `get` (cadre `PlayerCounters`, CR 122.1). Le lecteur d'action
  de joueur et la liste quantifiée partagent un enfant
  (`recognize_player_counters_fragment`) : `{E}{E} [(two energy counters)]`
  (le rappel doit compter les symboles, CR 207.2) ou `N KIND counter(s)` par
  le nom de compteur partagé. Tous les sujets de joueur l'atteignent :
  impératif, `you`, `that player`, `its controller`, `target player`,
  `each opponent`. Abaissement : primitive moteur existante
  `addPlayerCounters` (poison et énergie déjà exécutés, CR 704.5c).
- `put … counter on OBJECT` reste au cadre de placement : la revendication
  exige un nom de compteur sans `on`/`from` après lui.
- Sonde : ligue 809 → 809, corpus 14329 → 14363 (+34), 0 perte, IR des
  cartes supportées inchangée. Preuves : cartes réelles (Pit Scorpion,
  Pistus Strike, Aethertorch Renegade), recombinaisons inédites
  (`each opponent gets two rad counters`, `you get an experience counter`),
  quasi-échec (rappel qui ne compte pas les symboles), exécution
  (`effect_subject_runtime.rs` : poison au joueur de l'événement, énergie
  au contrôleur).
