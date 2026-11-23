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

### Étape 15 : `they` sujet joueur, joueur des dégâts d'une population

- `they` (CR 109.5, « they » singulier) nomme le joueur que nommerait
  `that player` : la liaison de joueur du programme, sinon le joueur de
  l'événement ; il prend la forme verbale plurielle. Lu par le même lecteur
  de sujet que les actions de joueur et la proposition de vie ; sans joueur
  lié, il n'est pas revendiqué (les antécédents d'objets pluriels gardent
  leurs lecteurs).
- Cadre : `a creature you control deals combat damage to a player` (par
  objet) publie le joueur blessé ; chaque enregistrement de dégâts du moteur
  porte déjà `triggeringPlayerId` (CR 510.2).
- Sonde : ligue 809 → 809, corpus 14363 → 14371 (+8, dont Sheoldred, the
  Apocalypse et Larceny), 0 perte, IR des cartes supportées inchangée.
  Preuves : IR identique entre `they` et `that player` (cartes réelles et
  recombinaison inédite après `target player`), quasi-échecs sans joueur
  lié, exécution (le joueur qui pioche perd la vie).

### Étape 16 : `For each X, ACTION.` préposé

- Introducteur structurel de phrase, lu comme `Until …,` après le refus
  des lecteurs ordinaires : le dénombrable est l'enfant partagé
  (`recognize_for_each_count`, ou `OBJECTS VERBed this way` résolu contre
  les actions du programme une fois l'action lue) et multiplie la quantité
  de l'unique action de sa phrase par `apply_for_each_multiplier`, comme le
  multiplicateur postposé. Une seconde action dans la phrase ou une action
  sans quantité littérale échoue en `sequence.for-each.preposed-action`.
- Une population de joueurs (`For each opponent, …`) n'est pas un
  dénombrable : c'est le cadre distributif (étape suivante).
- Sonde : ligue 809 → 809, corpus 14371 → 14375 (+4), 0 perte, IR des
  cartes supportées inchangée. Preuve : IR identique à la forme postposée
  (déjà exécutée), quasi-échecs (deux actions, population de joueurs).
- Restes mesurés : jetons nommés (`create a Treasure token`) sans quantité
  numérique, compteurs `on it` d'une source morte, `card exiled this way`.

### Étape 17 : proposition négative `OBJECT doesn't untap during …`

- Sujet permanent partagé (`that creature`, `it`, `that land`, cible) suivi
  de la négation accordée au nombre du sujet (`doesn't` / `don't`), du lemme
  `untap` du lexique et de la fenêtre `during its controller's next untap
  step` ou `during its controller's untap step for as long as you control
  SOURCE` (durée partagée, CR 611.2b). Nœud `UntapRestriction`, revendiqué
  par le lecteur d'action de permanent après son sujet ; abaissement vers la
  primitive moteur existante `installUntapRestriction` (CR 502.3), une par
  objet du sujet.
- Sonde : ligue 809 → 811 (+2 : Ty Lee, Chi Blocker ; Skyline Cascade),
  corpus 14375 → 14421 (+46), 0 perte, IR des cartes supportées inchangée.
  Preuves : cartes réelles (Frost Lynx, Chill of the Grave, Tidebinder Mage),
  recombinaison inédite après un changement de caractéristiques,
  quasi-échecs (accord, autre pas de dégagement), exécution (`skipNextUntap`
  installé sur la cible engagée).
- Reste : `Those creatures don't …` après une cible plurielle
  (antécédent pluriel non lié).

## Phase 6, suite (après #56)

Revue de #55 (`reference.rs`) : `the NOUN` (`sacrifice the permanent`,
CR 702.32a) résout comme un pronom vers l'antécédent source du résolveur
de liaisons. Conservé tel quel : la description définie ne nomme jamais un
objet nouveau et ne lit que l'antécédent source déjà prouvé ; elle ne
couvre pas les antécédents ciblés (ceux-ci passent par l'anaphore nominale
`that NOUN`, validée contre le sélecteur déclaré).

### Étape 18 : la carte qu'un déplacement met sur le champ de bataille

- CR 400.7 : l'effet qui met une carte sur le champ de bataille retrouve le
  permanent qu'elle devient. Une carte choisie, une cible de carte unique
  ou la dernière action d'une séquence facultative locale devient
  l'antécédent `MovedOntoBattlefield` (liaison `moveCards.bind` existante,
  lue par `singleBoundObject`). Le prédicat de carte déclaré sert de
  sélecteur pour l'anaphore nominale (`that creature`), comme pour une
  cible.
- Sonde : ligue 815 → 816 (+1 : Pre-War Formalwear), corpus 14562 → 14567
  (+5), 0 perte, aucune IR modifiée hors des cartes gagnées. Preuves : IR
  identique entre `it` et `that creature`, recombinaison (compteur sur la
  carte rendue), quasi-échecs (nominal non impliqué, plusieurs cartes),
  exécution (le compteur va sur le permanent rendu).
- Restes : la permission de programme `You may put … . That creature …`
  (deux étapes de programme : l'antécédent ne traverse pas la frontière
  d'étape) et les retards `Exile it at the beginning of the next end step`.

### Étape 19 : cadre réflexif `When you do,` dans le corps d'un sort

- Le corps d'un sort devient un programme d'effets dès qu'une phrase
  quelconque est suivie d'un connecteur réflexif ; le connecteur est lu par
  `outcome_at` partagé (`When you do`, `When they do`, `When that player
  does`), pas par une suite de mots. Une phrase réflexive postérieure ouvre
  une étape de programme comme un bloc de coût facultatif
  (`later_reflexive_action_block`). Un `Then` initial de phrase avant `you
  may` est lu (CR 608.2c : l'ordre des étapes l'exprime déjà).
- Abaissement inchangé (`performAction` + capacité réflexive, CR 603.12) ;
  une instruction que le moteur ne mesure pas comme une seule exécution
  échoue en `lower.program.reflexive-action.action`.
- Sonde : ligue 816 → 816, corpus 14567 → 14571 (+4 : Faebloom Trick,
  Thrilling Discovery, Lithobraking, Invasion of Ergamon), 0 perte, IR des
  cartes supportées inchangée. Preuves : même programme pour le sort et la
  capacité déclenchée, permission `Then you may` suivie du cadre,
  quasi-échec (`Scry 1. When you do,`).
- Restes : corps des capacités activées (séquence, pas programme),
  instructions abaissées en plusieurs primitives (scry, sacrifice par un
  autre joueur), antécédent `it` transporté dans la capacité réflexive.

### Étape 20 : `For each PLAYER, …` cadre distributif explicite

- Une population de joueurs après `For each` est le cadre distributif
  (CR 608.2c), jamais un dénombrable. Un corps qui déclare des cibles les
  déclarerait par joueur à la mise sur la pile (CR 601.2c) : le moteur ne
  déclare qu'une fois, échec `sequence.for-each-player.per-player-targets`
  (52 capacités, 33 cartes dont c'est le seul bloqueur). Sans cible, le corps
  n'a pas encore d'abaissement en boucle de joueurs (`forEachPlayer`
  existe) : `sequence.for-each-player.body` (29 capacités, 20 cartes à
  bloqueur unique, surtout des copies de jetons attaquant « that player »).
- Sonde : ligue 816 → 816, corpus 14571 → 14571, 0 perte, aucune IR
  modifiée ; seuls les diagnostics changent.

## Phase 7 : un emplacement d'exécutant pour toute action

### Étape 21 : l'exécutant (`performer`)

- Le sujet d'une proposition qui nomme un autre joueur que le contrôleur
  (`that player`, `they`, `its owner|controller`, `that NOUN's owner`,
  `defending player`, `each player|opponent|other player`) est lu par la
  proposition (`recognize_performer`), qui vérifie l'accord du verbe par le
  lexique. Le prédicat est ensuite lu par les lecteurs d'action ordinaires
  comme l'instruction de cet exécutant, à la deuxième personne : le verbe à
  la forme de base et les pronoms de l'exécutant de sa phrase (`they`,
  `their`) en `you`/`your` sur une vue des mêmes jetons (indices et étendues
  d'origine, diagnostics inchangés). Aucun lecteur par verbe.
- IR : chaque primitive produite par l'action reçoit `performer` (un joueur
  singulier) ; une population devient `forEachPlayer` dont le corps porte
  `performer: boundValue` (CR 101.4). Moteur (`effect_performer.rs`) : la
  répartition générique résout l'exécutant une fois et exécute la primitive
  existante comme l'instruction de ce joueur (contrôleur de la résolution
  remplacé) : ses choix, `you`, `your graveyard` et `they control` sont les
  siens. Aucune primitive ne connaît l'exécutant.
- Lexique : `choose`, `shuffle`, `search`.
- Sonde : ligue 816 → 816, corpus 14749 → 14780 (+31 : Exhume, Ulamog the
  Ceaseless Hunger, Goblin Spymaster, Mana Breach, Curfew, Empty the
  Catacombs…), 0 perte, IR des cartes supportées inchangée. Preuves : IR
  égale à l'instruction impérative plus `performer`, boucle de population,
  recombinaison inédite (`each opponent creates a Treasure token`),
  quasi-échecs (accord, possessif sans objet), exécution (le jeton va au
  joueur de l'événement ; chaque joueur remet sa propre carte).

### Étape 22 : l'exécutant continue sur les verbes de sa phrase

- Après une action dont un autre joueur est l'exécutant (lue par
  l'emplacement d'exécutant, par le lecteur d'action de joueur ou par la
  liste quantifiée), un verbe sans sujet de la même phrase garde cet
  exécutant (`each opponent mills two cards, then creates …`, `its
  controller reveals …, then puts …`). Chaque action d'une population reste
  sa propre boucle (CR 608.2c, 101.4). La continuation ne franchit pas la
  frontière de phrase.
- Sonde : ligue 816 → 816, corpus 14780 → 14784 (+4 : Mirko Vosk, Clear the
  Land, Destroy the Evidence, Malboro), 0 perte, IR des cartes supportées
  inchangée.

## Phase 8 : durées indéfinies, actions retardées sur objets liés

### Étape 23 : durée indéfinie

- `EffectDurationNode::Indefinite` (CR 611.2a) : une proposition d'effet
  continu qui se termine sans durée (`That creature gains haste.`) a une
  durée indéfinie, lue par la grammaire de durée partagée
  (`recognize_trailing_or_supplied_duration`). Chaque abaissement décide :
  l'octroi de mot-clé a la primitive moteur `duration: permanent`, attachée à
  l'objet (un nouvel objet ne la garde pas, CR 400.7) ; les autres (`gets
  +N/+N`, `becomes`) échouent explicitement.
- Sonde : ligue 818 → 818, corpus 14825 → 14831 (+6 : Fated Return, Grave
  Upheaval, les quatre Chimera), 0 perte, IR des cartes supportées
  inchangée. Preuves : retour puis octroi, recombinaison ciblée, quasi-échec
  (statistiques), exécution (le mot-clé n'a pas d'échéance).

### Étape 24 : actions de zone sur un permanent lié, retardées

- `exile|sacrifice it|that creature|the creature` sur un permanent lié autre
  que la source (la carte qu'un déplacement a mise sur le champ de bataille,
  une cible de permanent déclarée) est l'action de zone de ce permanent :
  le nœud de référence de destruction porte désormais l'action
  (`Destroy|Exile|Sacrifice`), abaissée vers `destroyPermanent`,
  `exilePermanent`, `sacrificePermanent` (support moteur du `singleBoundObject`
  ajouté pour le sacrifice). Ce n'est jamais une collection de cartes.
- Une échéance différée (`at the beginning of the next end step`) déplace
  l'action dans le déclencheur retardé existant, qui capture l'identité de
  l'objet lié à sa création (CR 603.7c).
- Moteur : `singleBoundObject`/`boundObject` lu par un déclencheur retardé
  retrouve l'identité capturée dans ses décisions ; sans cela l'action
  retardée ne faisait rien à l'exécution (règle faussement exécutable).
- Sonde : ligue 818 → 819 (+1 : Goryo's Vengeance), corpus 14831 → 14840
  (+9 : Apprentice Necromancer, Treacherous Urge, Footsteps of the Goryo,
  Postmortem Lunge…), 0 perte, IR des cartes supportées inchangée. Preuves :
  exil et sacrifice retardés avec capture, quasi-échec (carte rendue à la
  main), exécution (la carte rendue est exilée à l'étape de fin).

### Étape 25 : l'antécédent d'un déploiement facultatif traverse les phrases

- Le déploiement facultatif (`you may put a creature card from your hand
  onto the battlefield`) a le même antécédent `MovedOntoBattlefield` qu'une
  carte choisie ou ciblée ; son prédicat de carte valide `that creature`.
  Le sélecteur de l'antécédent est cherché vers l'arrière dans les actions
  du programme (il survit à `That creature gains haste.`), et `the creature`
  (description définie) se lit comme `that creature` dans l'action de zone
  sur un permanent lié.
- Le déplacement ne lie le résultat que si une primitive ultérieure le lit
  (marqueur de lowering transitoire) : l'IR des déploiements sans
  référence ultérieure est inchangée.
- Sonde : ligue 819 → 820 (+1 : Sneak Attack), corpus 14840 → 14847 (+7 :
  Through the Breach, Coalstoke Gearhulk, Arms Race, Incandescent
  Soulstoke…), 0 perte, IR des cartes supportées inchangée. Preuves :
  Sneak Attack (liaison, hâte, sacrifice retardé), recombinaison (compteur),
  absence de liaison sans lecteur, exécution (hâte puis sacrifice à l'étape
  de fin).

### Étape 26 : garde du `you` écrit dans la proposition d'un exécutant

- Un `you`/`your` écrit dans la proposition d'un autre joueur nomme le
  contrôleur de la capacité ; la lecture comme instruction de l'exécutant le
  lirait comme l'exécutant. Cette lecture ambiguë échoue en
  `clause.performer.controller-reference`. Les capacités citées (CR 113.1)
  sont leur propre portée : ni la garde ni le passage à la deuxième personne
  ne s'y appliquent (Goblin Spymaster garde son `Creatures you control`).
- Sonde : ligue 820 → 820, corpus 14847 → 14847, 0 perte, aucune IR
  modifiée : aucune carte supportée ne contenait ce cas.

### Étape 27 : permission d'un exécutant (`each opponent may …`)

- `PLAYER may VERB …` est la permission propre de l'exécutant (CR 608.2d) :
  le dernier mot du sujet se lit `you`, le verbe reste nu, et le programme
  facultatif existant (`recognize_optional_sequence`) lit la phrase ;
  l'exécutant (singulier ou boucle de population) est posé sur
  `optionalEffects`, dont le moteur adresse déjà le choix au joueur résolu.
- Sonde : ligue 826 → 827 (+1 : Fecundity), corpus 15080 → 15089 (+9 :
  Hunted Wumpus, Harbor Guardian, Vex, Brood Sliver…), 0 perte, IR des
  cartes supportées inchangée.

### Corrections de revue (#63)

- Déclencheur retardé : chaque objet capturé voyage avec l'identité de
  l'objet qu'il nommait (`captured_binding.rs`, clé `BINDING#identities`) ;
  le déclencheur ne le retrouve que tant qu'il est le même objet (CR 400.7,
  603.7c), comme la contrepartie de combat. Le repli de `resolve_target` ne
  lit que ce contexte de liaison capturée.
- Sacrifice d'un permanent lié : seul son contrôleur le sacrifie
  (CR 701.21a) ; un permanent volé n'est pas sacrifié par l'exécutant.
- Garde du `you` : ne lit que les jetons consommés par l'action exécutée
  (`, then you draw a card` reste l'action du contrôleur).
- La permission d'un joueur singulier (`that player may`) garde son lecteur
  de programme (étape 12) ; seule une population passe par l'exécutant :
  une seule représentation par forme.

### Retrait de l'étape 27 (revue #63)

- La permission d'une population (`each player|opponent may VERB …`) n'est
  plus lue par l'emplacement d'exécutant : sa boucle met les cartes sur le
  champ de bataille un joueur après l'autre, alors que CR 101.4 les rend
  simultanées, et elle n'avait pas de preuve d'exécution. Elle revient en
  échec explicite (`clause.verb.unsupported`) jusqu'à ces preuves.

### Suites de revue (#63, non bloquantes)

- La liaison de la carte déployée passe par l'allocation unique de
  `movedOntoBattlefield` (`CardCollectionMemory::allocate_moved_onto_battlefield`) ;
  le marqueur transitoire a disparu ; un second producteur échoue en
  `lower.optional-card-deployment.binding`.
- Un seul lecteur pour `destroy|exile|sacrifice` d'une référence liée
  (`recognize_referenced_permanent_action`) ; la description définie
  (`the creature`) est lue par le résolveur partagé
  (`recognize_definite_object_reference`), plus de réécriture de jetons.
- Durée indéfinie : une restriction de combat sans durée (`Target creature
  can't block.`) donne `cantBlock` permanent, appliqué par l'énumération des
  bloqueurs ; `This creature can't block.` reste statique ; un octroi sans
  durée au sort lui-même échoue en `lower.keyword-grant.indefinite-spell`.
- Preuves : antécédent ciblé non différé (`exile|sacrifice it`, `the
  creature`), forme différée sur une cible refusée, déploiement vide sans
  effet à l'étape de fin.
