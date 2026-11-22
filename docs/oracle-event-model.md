# Modèle d'événement générique du moteur (21 novembre 2026)

Ce document fixe la cible du moteur pour les déclencheurs et l'état de sa
première étape (famille combat). Il complète `oracle-clause-layer.md` : la
couche de proposition lit un motif d'événement par lemme ; ce document décrit
comment le moteur l'évalue.

## Diagnostic

Le moteur compare aujourd'hui chaque action à des `kind` d'événement en forme
de phrase (`declaredAttacker`, `permanentBecameBlocked`,
`controlledCreatureBecameBlocked`, `damageDealtBySource`, …). Chaque `kind` a
son test d'admission dans `triggered_event_supported` et son test de
correspondance écrit à la main au point où l'action a lieu (`src/engine/mod.rs`,
`source_damage_observation.rs`, `counter_placement.rs`, …). Conséquences :

- un modificateur (`alone`, `and isn't blocked`, `for the first time each
  turn`, un bloqueur qualifié) exige du code moteur par `kind` ;
- un même concept est lu avec des vues différentes selon le `kind` (définition
  imprimée, état du permanent, caractéristiques effectives) ;
- des champs sont admis sans être évalués (`creatureAttacked` acceptait
  `attackingAlone` sans le lire) ;
- l'analyseur reconnaît déjà ces modificateurs mais doit échouer en
  `trigger.event.engine-gap` (73 cartes du corpus au départ de cette branche).

## Cible

### Enregistrement d'événement

Chaque action du moteur publie **un** enregistrement typé, au moment où elle a
lieu :

| Champ | Contenu |
|---|---|
| lemme | l'action (`attack`, `block`, `becomeUnblocked`, puis `dealDamage`, `move`, `cast`, `putCounters`, …) |
| acteur | le joueur qui accomplit l'action (attaquant qui déclare, joueur qui déclare le blocage, lanceur, …) |
| objets par rôle | sujet, objet, attaquant, bloqueur, cible, source |
| zones | origine et destination |
| quantités | montant de dégâts, nombre de marqueurs, … |
| drapeaux | `attacking_alone`, `declared_attacker`, premier blocage de l'attaquant ou du bloqueur, dégâts de combat |
| ordinal | rang de cet événement pour ce lemme et cet objet (ou cet acteur) dans le tour |

L'ordinal et les drapeaux sont calculés par l'émetteur, qui seul connaît la
déclaration complète (CR 506.5 : une créature attaque seule si elle est la
seule déclarée comme attaquante).

### Motif d'événement

Un déclencheur porte la même forme avec des prédicats à la place des valeurs :

- lemme et **rôle** occupé par le sujet du déclencheur (`blocks` : sujet
  bloqueur ; `becomes blocked` : sujet attaquant du même enregistrement de
  blocage) ;
- **relation** du sujet à la source : la source elle-même, le permanent auquel
  elle est attachée, un permanent contrôlé par son contrôleur ;
- **requêtes d'objets** sur le sujet et sur la contrepartie, évaluées par les
  évaluateurs de filtres existants du moteur ;
- relations de joueur, comparaisons de quantités ;
- **modificateurs** : `alone`, `declaredAttacker` (`attacks and isn't
  blocked`), une fois par combat (`blocks`, `becomes blocked`, CR 509.3a,
  509.3c), une fois par paire (`blocks a creature`, `becomes blocked by a
  creature`, CR 509.3b, 509.3d), `one or more` (une fois par combat, à la
  première contrepartie qui satisfait la requête), ordinal du tour.

Un seul correspondant évalue n'importe quel motif contre n'importe quel
enregistrement. Un attribut que le correspondant ne sait pas évaluer rend le
motif non admis (règle non exécutable) ; il n'est jamais ignoré.

### Forme IR

Les nouveaux motifs s'écrivent :

```json
{"kind": "eventPattern", "lemma": "block",
 "subject": {"role": "attacker", "object": {"kind": "self"}},
 "counterpart": {"role": "blocker", "where": FILTRE, "oneOrMore": true},
 "attackingAlone": true, "declaredAttacker": true, "turnOrdinal": 1}
```

`subject.object` vaut `{"kind":"self"}`, `{"kind":"attachedPermanent",
"attachment":{"kind":"self"}}` ou `{"kind":"controlledPermanent", "player":
contrôleur de la source, "where": FILTRE}`. Les champs inconnus rendent le motif
non admis.

### Correspondance des anciens `kind`

Les anciens `kind` restent des **alias** lus comme motifs : l'IR émise des
cartes déjà supportées ne change pas. Chaque alias garde la vue d'objet de son
ancien test pour garder un comportement identique :

| Ancien `kind` | Lemme | Rôle du sujet | Relation | Filtres / modificateurs |
|---|---|---|---|---|
| `declaredAttacker` | attack | attaquant | source | — |
| `controlledCreatureDeclaredAttacker` | attack | attaquant | contrôlé | `where` effectif, `saddledOnly`, `faceDownOnly` |
| `attachedPermanentDeclaredAttacker` | attack | attaquant | attaché | `attackingAlone` |
| `declaredBlocker` | block | bloqueur | source | une fois par combat |
| `controlledCreatureDeclaredBlocker` | block | bloqueur | contrôlé | `where` (état du permanent), une fois par combat |
| `attachedPermanentDeclaredBlocker` | block | bloqueur | attaché | une fois par combat |
| `permanentBecameBlocked` | block | attaquant | source | une fois par combat |
| `controlledCreatureBecameBlocked` | block | attaquant | contrôlé | `where` (état du permanent), une fois par combat |
| `attachedCreatureBecameBlocked` | block | attaquant | attaché | contrepartie `where` (définition imprimée), par paire |
| `attachedCreatureBlocks` | block | bloqueur | attaché | contrepartie `where` (définition imprimée), par paire |

Les alias `controlled*` n'admettent plus que le contrôleur de la source comme
joueur : c'était la seule relation évaluée (l'ancien test acceptait toute
référence de joueur et l'ignorait) et la seule émise.

### Ordre de publication

Le correspondant est unique ; l'ordre d'empilement établi est conservé par un
**groupe de publication** dérivé du motif (sujet source, sujet contrôlé,
attachement, observateurs du bloqueur, observateurs de l'attaquant bloqué,
attachements de chaque combattant, attaquant non bloqué). Chaque point
d'émission parcourt les sources du champ de bataille dans l'ordre et publie
les règles dont un motif du groupe correspond à l'enregistrement. Les liaisons
de décision des alias (`triggeringPermanentId`, `triggeringPlayerId`,
`blockingPermanentId`) sont celles des anciens tests ; un motif générique de
blocage lie en plus `blockingPermanentId` et `blockedPermanentId`.

### Retrait des tests par `kind`

Pour une famille migrée : (1) les alias sont lus par `combat_event_pattern`,
qui remplace leurs bras dans `triggered_event_supported` ; (2) les tests écrits
à la main aux points d'émission sont remplacés par `combat_record_triggers` ou
`rule_matches_combat_record` ; (3) un `kind` sans producteur est supprimé
(`creatureAttacked`). Une famille suivante ne crée pas de nouveau `kind` : elle
ajoute son lemme à l'enregistrement et au motif.

## Information de dernier état et moment d'évaluation

- CR 603.2 : un déclencheur se déclenche quand l'événement a lieu ; le motif
  est évalué immédiatement à la publication de l'enregistrement, avant toute
  résolution. Les filtres lisent l'état courant, qui est l'état de l'événement
  (les déclencheurs sont collectés avant la fenêtre de priorité).
- CR 603.10a : les déclencheurs de départ du champ de bataille « regardent en
  arrière ». Pour les familles de changement de zone, l'enregistrement portera
  l'instantané de l'objet avant le déplacement (les instantanés de
  `source_lki.rs` et `death_trigger_batch.rs`) ; le correspondant évaluera les
  requêtes sur cet instantané, pas sur l'objet dans sa nouvelle zone. La
  famille combat n'a pas de départ : ses objets sont sur le champ de bataille.
- L'ordinal est calculé depuis le journal du tour (`attackerDeclared`,
  `blockerDeclared`), où l'événement courant est déjà inscrit.
- `becomeUnblocked` est publié une fois tous les blocages déclarés (CR
  509.1h), avant la fenêtre de priorité ; seul un attaquant déclaré satisfait
  `declaredAttacker` (une créature mise sur le champ de bataille attaquante n'a
  pas attaqué, CR 508.4).

## Étape 1 réalisée : famille combat

- `src/engine/event_model.rs` : `CombatEventRecord`, `CombatEventPattern`,
  `combat_event_pattern` (alias + `eventPattern`), `combat_event_matches`,
  enregistrements d'attaque, de blocage et d'attaquant non bloqué, publication
  par groupe.
- Points d'émission migrés : déclaration des attaquants (attaquant lui-même,
  observateurs contrôlés, attachements), déclaration de chaque blocage
  (observateurs du bloqueur, de l'attaquant bloqué, attachements),
  attaquants non bloqués (nouveau).
- Analyseur : la contrepartie d'un blocage est une requête typée
  (`EventCounterpartNode`, `one or more` inclus), partagée par un `blocks` nu
  dans `blocks or becomes blocked by QUERY`. `alone`, `and isn't blocked`, la
  contrepartie et `for the first time each turn` (attaque, blocage) sont
  abaissés en `eventPattern`. Restent en `trigger.event.engine-gap` :
  l'ordinal sur `becomes blocked` et `isn't blocked`, une contrepartie avec
  contrôleur ou caractéristique numérique, `and isn't blocked` après un autre
  événement qu'une attaque seule.
- Correction intentionnelle : `becomes blocked by a creature` se déclenchait
  une fois par combat ; c'est un événement par bloqueur (CR 509.3d). L'IR de
  ces cartes passe de `permanentBecameBlocked` à `eventPattern` avec
  contrepartie.

### Mesures (sondes ligue/corpus, base b84e6c7)

| Mesure | Base | Après |
|---|---|---|
| Ligue, cartes supportées | 790/1664 | 790/1664 (0 gagnée, 0 perdue, 0 IR changée) |
| Corpus, cartes supportées | 13608/35770 | 13633/35770 (+25, 0 perdue) |
| Corpus, cartes en `trigger.event.engine-gap` | 73 | 1 (Arrogant Bloodlord : contrepartie numérique) |
| Corpus, IR changée | — | 31 cartes : 25 gagnées, 4 corrigées (`becomes blocked by a creature`), 2 toujours en échec ailleurs |

Les 47 autres cartes de l'ancienne file `engine-gap` échouent maintenant plus
loin, sur une grammaire de corps distincte : référence à la contrepartie
(`that creature`, 12 cartes), `assigns no combat damage`, `until end of
combat`, `defending player` hors des lecteurs qui l'acceptent, etc.

### Tests par `kind` retirés et restants

- Retirés : les bras de `triggered_event_supported` de `declaredAttacker`,
  `declaredBlocker`, `permanentBecameBlocked`,
  `controlledCreatureBecameBlocked`, `controlledCreatureDeclaredAttacker`,
  `controlledCreatureDeclaredBlocker`, `attachedPermanentDeclaredAttacker`,
  `attachedPermanentDeclaredBlocker`, `attachedCreatureBecameBlocked`,
  `attachedCreatureBlocks` et le `kind` mort `creatureAttacked` ; les tests
  écrits à la main de ces `kind` aux deux points d'émission.
- Restants (famille combat, agrégats ou règles intégrées) :
  `controlledCreaturesAttacked`, `attackDeclaredWithSourceOrCommander`,
  `opponentSmallCreatureBlocks`, `permanentAttacks` (déclencheurs retardés),
  les règles synthétisées pour exalted, annihilator, flanking et le porteur de
  l'Anneau, et le test de cause d'attaque de `trigger_multiplicity.rs`.

## Étape 1b réalisée : référence au combattant et identité (CR 400.7)

- Analyseur : un événement de blocage dont chaque branche a une contrepartie
  (`blocks a creature`, `becomes blocked by X`, `blocks or becomes blocked
  by X`) lie l'autre combattant. Sans antécédent local, `that creature`
  désigne cette référence typée (`TypedObservedEventObject::CombatCounterpart`,
  IR `{"kind":"combatCounterpart"}`) ; `it` reste la source. Le lecteur
  partagé de `destroy` accepte une référence liée (`destroy that creature`),
  avec `It can't be regenerated` (même lecteur que `They can't…`).
- Moteur : un motif générique de blocage lie `combatCounterpartId` (le
  combattant opposé au rôle du sujet, selon la branche correspondante) et son
  incarnation. À la résolution la référence ne désigne l'objet que s'il est
  toujours la même incarnation (CR 400.7) ; sinon l'action ne fait rien.
- L'ordinal par tour ne compte que l'incarnation courante : `attackerDeclared`
  et `blockerDeclared` journalisent `objectIdentity`, et un objet revenu sur
  le champ de bataille recommence son compte.
- Sondes : ligue inchangée ; corpus 13633 → 13639 (+6 : Ogre Leadfoot,
  Phyrexian Slayer, Phyrexian Reaper, Sylvan Basilisk, Quagmire Lamprey,
  Flailing Drake), 0 perdue, 8 IR changées (les 6 gagnées, et Witherscale
  Wurm, Evil Eye of Urborg dont une capacité de blocage compile désormais,
  une autre échouant ailleurs).
- Restent bloquées plus loin : 7 cartes `destroy that creature at end of
  combat` (Cockatrice, Dread Specter, …) attendent un déclencheur retardé
  `at end of combat` ; Battering Ram (`that Wall`, nominal de sous-type, et
  banding) ; Catacomb Dragon (`non-Dragon`).

## Étape 2 réalisée : famille dégâts

- `src/engine/damage_events.rs` : `DamageEventRecord` (source, contrôleur de
  la source au moment des dégâts — information de dernier état fournie par la
  préparation des dégâts —, destinataire joueur ou permanent, montant,
  combat, événement simultané), `DamageEventPattern`, un seul correspondant
  `damage_event_matches`. Chaque dégât infligé à un destinataire publie un
  enregistrement (`enqueue_source_damage_observers`, appelé par le chemin
  unitaire et par le lot de dégâts aux joueurs).
- Alias lus comme motifs, IR inchangée : `damageDealtBySource`,
  `controlledPermanentDealtDamage`, `permanentDealtDamage`,
  `combatDamageToPlayer`, `attachedPermanentCombatDamageToPlayer`. Les groupes
  de publication gardent les points d'émission et liaisons établis.
- Nouveau motif `{"kind":"eventPattern","lemma":"dealDamage","source":…}` :
  sans destinataire, il observe tout l'événement de la source — un seul
  déclenchement par source et événement simultané, montant total (`that
  much`). L'événement simultané est le lot d'événements existant
  (`event_batch.rs`) : une instruction, et désormais l'étape de dégâts de
  combat (CR 510.2). Un déclenchement déjà publié pour la même source dans le
  même événement voit son `damageAmount` augmenter.
- Analyseur : `SOURCE deals [combat] damage` sans destinataire est abaissé en
  ce motif (au lieu de `trigger.dealt-damage.recipient-support`).
- `kind` sans producteur retirés avec leurs tests à la main :
  `selfDealtDamage`, `sourceDealtDamageAtLeast`,
  `sourceDealtDamageToPermanent`, `attachedPermanentDealtDamage`,
  `controlledSourceDealtExactDamage` (ne réagissait qu'à 2 dégâts),
  `controlledSourceDealtExactDamageToController`,
  `opponentCombatDamageDuringControllerTurn`.
- Restent par `kind` (agrégats par joueur ou groupe) : `opponentDealtDamage`,
  `damageDealtToOpponent`, `combatDamageReceived`,
  `controlledCreaturesCombatDamageToPlayer` ; et la file `one or more SOURCES
  deal damage` (agrégat sur plusieurs sources, non prouvé).
- Sondes : ligue inchangée ; corpus 13639 → 13656 (+17 : Armadillo Cloak,
  Spirit Link, Vampiric Link, Exalted Angel, Drinker of Sorrow, Five-Alarm
  Fire, …), 0 perdue, 30 IR changées (17 gagnées, 13 dont la capacité de
  dégâts compile et une autre échoue ailleurs).

## Étape 3 réalisée : famille changements de zone

- `src/engine/zone_change_events.rs` : `ZoneChangeRecord` (l'objet tel qu'il
  existait en dernier dans la zone quittée — information de dernier état —,
  zone d'origine si connue, zone d'arrivée, cause de la mort),
  `ZoneChangePattern`, un seul correspondant `zone_change_matches`.
- Trois points de publication, un même enregistrement :
  - mort d'un permanent (`Death`) : sources capturées avant les morts
    simultanées des actions d'état (`death_trigger_batch.rs`) ;
  - départ du champ de bataille (`Departure`) : sources capturées avant le
    départ, donc « regard en arrière » (CR 603.10a), y compris la source qui
    part elle-même ;
  - arrivée au cimetière (`GraveyardArrival`) : les capacités de la carte
    elle-même (`put into a graveyard from anywhere`), quel que soit le chemin
    (mort, meule, défausse, sort résolu ou contrecarré).
- Le contrôleur, la force, l'endurance et les marqueurs lus par les motifs et
  liés aux décisions sont ceux de l'instantané : une créature volée qui meurt
  déclenche les observateurs de son dernier contrôleur, avec sa dernière
  force (preuve runtime).
- Alias lus comme motifs, IR inchangée : `permanentDied` (source ou filtré,
  vue imprimée, `player`, `nontoken`, `reason`, `hadCounter`,
  `hadAnyCounters`, `damagedBySourceThisTurn`, `excludeSource`,
  agrégation), `attachedPermanentDied`, `permanentLeftBattlefield`,
  `controlledPermanentLeftBattlefield`.
- Nouveau motif `{"kind":"eventPattern","lemma":"move","subject":{"object":
  …},"from":"battlefield","to":ZONE}` ; sujet `self` ou `permanents`
  (contrôleur quelconque, relation de joueur, `where` lu sur l'instantané,
  `excludeSource`). Sans `from` et vers le cimetière, sujet `self` : la
  capacité de la carte à son arrivée.
- Analyseur : un départ d'un permanent de n'importe quel contrôleur ou d'un
  adversaire (`Whenever a land is put into a graveyard from the battlefield`,
  `Whenever an artifact … leaves the battlefield`) et `SOURCE is put into a
  graveyard from anywhere` sont abaissés en ce motif. Restent explicites :
  `one or more` départs, `your graveyard` (relation de propriétaire de la
  destination), un prédicat numérique sur l'objet parti.
- `kind` sans producteur retirés : `smallControlledCreatureDied`,
  `opponentCreatureDied`, `controlledCreatureDiedOrExiled`,
  `sourceDiedOrExiled`, `selfEnteredGraveyard`.
- Restent par `kind` : entrées (`enterBattlefield`, `permanentEntered`, vues
  d'entrée propres à `entry_observation.rs`), `permanentSacrificed` (sacrifice
  avec prédicat d'identité stable seulement : 62 cartes du corpus attendent
  les caractéristiques effectives dans l'instantané), `cardsEnteredGraveyard`
  et `cardsLeftGraveyard` (agrégats par joueur), `tokenCreated`.
- Sondes : ligue inchangée ; corpus 13656 → 13666 (+10 : Disciple of the
  Vault, Moriok Rigger, Molder Beast, Femeref Enchantress, Fangren Marauder,
  …), 0 perdue, 16 IR changées. Les cartes `from anywhere` (titans Eldrazi,
  Purity, Vigor…) échouent désormais dans leur corps (`its owner shuffles`,
  `shuffle it into`), plus sur l'événement.

## Étape 4 réalisée : instantané des caractéristiques effectives, sacrifice

- `src/engine/object_snapshot.rs` : `EffectiveCharacteristics` (types,
  sous-types, « tous les types de créature », couleurs, supertypes
  légendaire/de base/neige, jeton, capacités mot-clé), calculées avec tous
  les effets continus appliqués. `capture_pending_source_lki`, appelé par
  chaque sortie centrale du champ de bataille pendant que l'objet y est
  encore, enregistre cet instantané par incarnation (CR 608.2h, 603.10a).
  Dans un lot simultané (sacrifice de plusieurs permanents), tous les
  instantanés sont pris avant le premier départ.
- Le correspondant de la famille changements de zone lit d'abord
  l'instantané ; un atome qu'il ne capture pas (caractéristique numérique,
  état) ou un objet sans capture garde la vue établie du motif.
- `permanentSacrificed` est lu comme motif de déplacement (cause
  `sacrificed`, observateurs capturés avant le départ), IR inchangée.
- Analyseur : un sacrifice accepte tout prédicat sur les caractéristiques
  capturées (`an artifact`, `a Clue`, `a black creature`, `a Clue or Food`,
  `another creature`) ; une caractéristique numérique reste explicite
  (`trigger.sacrifice.numeric-snapshot-support`).
- Correction de règle assumée (CR 603.10a) : `permanentDied` filtré et les
  départs lisent désormais les types/couleurs effectifs au moment du départ
  au lieu de la définition imprimée (un terrain animé qui meurt est une
  créature qui meurt). Le résultat ne change que si les caractéristiques
  effectives diffèrent des imprimées.
- Sondes : ligue 790 → 792 (+2 : Smothering Abomination, Crime Novelist) ;
  corpus 13666 → 13692 (+26 : Tireless Tracker, Gleaming Geardrake,
  Thraximundar, Body Dropper, Savra, …), 0 perdue, 45 IR changées dont 0
  déjà supportée.

## Étape 5 réalisée : déclencheur retardé `at end of combat`, multiplicateurs

- Le cadre retardé existant (`installDelayedStepTrigger`) reçoit une étape
  `endOfCombat` (CR 511) ; `at end of combat` est lu par le même lecteur de
  moment que `at the beginning of the next end step`, en tête ou en fin de
  phrase. Aucun parent par carte : `DelayedTurnStep::runtime_step` donne le
  nom d'étape à tous les abaisseurs retardés.
- Le moteur déclenche les délais `endOfCombat` à l'étape de fin de combat,
  avant les exils de fin de combat, et ouvre une fenêtre de priorité si l'un
  d'eux a été posé (aucun changement pour les combats sans délai).
- Une référence d'événement lue par les actions retardées (l'autre
  combattant) est transportée à la création du délai avec son identité : le
  délai ne détruit que le même objet (CR 400.7).
- Multiplicateurs de déclenchement (CR 603.2d) : chaque copie d'un
  déclencheur de dégâts agrégé voit le total de l'événement.
- Sondes : ligue inchangée ; corpus 13692 → 13702 (+10 : Cockatrice, Dread
  Specter, Thicket Basilisk, Deathgazer, Infernal Medusa, Abomination, Rock
  Basilisk, Venomous Dragonfly, Tangle Asp, Tolarian Entrancer), 0 perdue,
  11 IR changées dont 0 déjà supportée.

## Étape 6 réalisée : famille lancement

- `src/engine/cast_events.rs` : `CastEventRecord` (lanceur, définition du
  sort, décisions de lancement, cibles, historique des sorts du lanceur dans
  le tour) et un seul correspondant `spell_cast_matches` pour `spellCast`. La
  relation du sujet est typée : un sort quelconque observé par un permanent,
  ou le sort source lui-même observé depuis la pile (`sourceIsSelf`,
  CR 601.2i). Correction : un permanent portant `When you cast this spell`
  ne se déclenche plus sur les autres sorts.
- L'ordinal par tour est un modificateur du motif : il compte les sorts du
  lanceur dans le tour qui satisfont le même prédicat (`your first instant
  or sorcery spell each turn`) ; l'historique ne garde que ligne de type et
  nom, d'où une restriction explicite des prédicats d'ordinal filtré.
- Analyseur : `When you cast this spell`, `Whenever you cast or copy …`
  (Magecraft : `oneOf` de `spellCast` et `spellCopied` sur le même prédicat)
  et l'ordinal filtré sont reconnus par le lecteur partagé ; le parent de
  champ de bataille historique laisse ces formes au programme générique.
- Restent : prédicats numériques du sort sur la pile (valeur de mana avec X),
  sorts qui ciblent (`a spell that targets`), origine `from your graveyard`.
- Sondes : ligue 792 → 795 (+3 : Witherbloom Apprentice, Artisan of Kozilek,
  Storm-Kiln Artist) ; corpus 13702 → 13730 (+28 : World Breaker, Archmage
  Emeritus, Eldrazi Obligator, …), 0 perdue, 57 IR changées dont 0 déjà
  supportée.

## Étape 7 réalisée : un attaquant quelconque (famille combat)

- Le motif d'attaque générique accepte un sujet `permanents` avec relation
  de contrôleur (aucune, contrôleur de la source, adversaire), prédicat et
  exclusion de la source : `Whenever a creature you control attacks`,
  `Whenever a creature attacks`, `Whenever another creature an opponent
  controls attacks` — un enregistrement et un déclenchement par attaquant
  (CR 508.3a). Le même groupe de publication que l'ancien
  `controlledCreatureDeclaredAttacker` garde l'ordre d'empilement ; un motif
  générique lie le joueur attaqué (CR 506.2) pour `defending player`.
- Sondes : ligue inchangée ; corpus 13730 → 13745 (+15 : Dromoka the
  Eternal, Utvara Hellkite, Linden, Sanctum Seeker, Leeching Sliver, …),
  0 perdue, 23 IR changées dont 0 déjà supportée.

## Étape 8 réalisée : cibles du sort lancé

- Le motif `spellCast` reçoit un modificateur `targets` : `{"only": bool,
  "anyOf": [ALTERNATIVE…]}`, chaque alternative étant la source
  (`{"kind":"self"}`, Heroic), un joueur dans une relation au contrôleur, ou
  un permanent par la requête partagée (`where`, relation de contrôleur).
  Le correspondant le lit sur les cibles de l'enregistrement de lancement
  (CR 115.1, 601.2c) : au moins une cible, ou toutes avec `only`.
- Analyseur : `that targets [only] T (or T)*` après la requête du sort,
  dans le lecteur partagé de lancement ; les cibles réutilisent la
  référence de source, le lecteur d'acteur et la requête d'objets. Aucun
  parent de cible : le parent de champ de bataille historique laisse ces
  formes au programme générique.
- Sondes : ligue inchangée ; corpus 13745 → 13798 (+53 : Hero of the Games,
  Akroan Crusader, Phalanx Leader, Arena Athlete, Lecturing Scornmage, …),
  0 perdue, 56 IR changées dont 0 déjà supportée.

### Audit des sorties du champ de bataille (risque d'instantané)

Toutes les sorties de zone du champ de bataille passent par les sorties
centrales qui appellent `capture_pending_source_lki` (donc l'instantané des
caractéristiques effectives) : `battlefield_departure.rs` (cimetière et
exil, utilisés par destruction, sacrifice, actions d'état), les retours en
main/bibliothèque de `mod.rs`, et les lots de destruction et de sacrifice.
Les deux seuls autres retraits d'un permanent de la liste du champ de
bataille ne sont pas des changements de zone : le changement de contrôleur
(`change_permanent_controller`, même objet, CR 108.4) et la configuration
déterministe d'une position de test. Le risque restant de l'instantané est
le recensement des mots-clés et sous-types candidats (règles imprimées et
modificateurs actifs).

## Étape 9 réalisée : origine et tour du lancement

- L'enregistrement de lancement porte la zone d'origine (`castSourceZone`,
  CR 601.2a) et le propriétaire du sort ; le motif lit `fromZone`,
  `fromZoneNot` (`anywhere other than your hand`), `fromOwnZone` (`your
  graveyard` : le sort appartient au contrôleur de la capacité, CR 400.3).
- `during your turn` / `during an opponent's turn` sont relatifs au
  contrôleur de la capacité (CR 500.1), quel que soit le lanceur
  (`Whenever an opponent casts a spell during your turn`). Aucun producteur
  antérieur de ces champs n'existait pour le lancement : pas de changement
  d'IR supportée.
- Sondes : ligue 795 → 797 (+2 : Sisterhood of Karn, Vega, the Watcher) ;
  corpus 13798 → 13814 (+16), 0 perdue, 32 IR changées dont 0 déjà
  supportée.

## Étape 10 réalisée : caractéristiques numériques du sort sur la pile

- Le prédicat d'un motif de lancement est évalué sur l'enregistrement : une
  comparaison numérique sur le candidat lit la valeur de mana avec le X
  annoncé (CR 202.3e), la force et l'endurance de la carte ; une propriété
  que l'enregistrement ne porte pas (nombre de couleurs, …) garde
  l'évaluateur de définition. L'analyseur exige une constante à droite.
- Le parent de champ de bataille historique laisse ces prédicats au motif
  générique au lieu d'échouer.
- Sondes : ligue 797 → 798 (+1 : Path of Annihilation) ; corpus 13814 →
  13833 (+19 : Screamer-Killer, Enraged Flamecaster, Ascendant Packleader,
  Bygone Bishop, …), 0 perdue, 29 IR changées dont 0 déjà supportée.

## Étape 11 réalisée : lots `one or more` des changements de zone

- Arrivée au cimetière : le motif `move` accepte un sujet `cards` (relation
  de propriétaire : un cimetière, le vôtre, celui d'un adversaire, CR 400.3)
  avec prédicat de carte ; `put into OWNER graveyard [from anywhere]`. Les
  observateurs du champ de bataille le lisent à chaque arrivée : un
  déclenchement par carte, ou un seul par événement simultané avec
  `aggregation: oneOrMore` (lot d'événements existant, CR 603.2c), lié aux
  cartes correspondantes (`triggeringCardIds`).
- Départs : `one or more PERMANENTS leave the battlefield` est le motif de
  départ agrégé, réclamé une fois par source, règle et événement simultané.
- Reste explicite : l'origine `from your library` (l'arrivée ne porte pas sa
  zone d'origine), les lots de blocage et de marqueurs.
- Sondes : ligue 798 → 800 (+2 : Sand Scout, The Gitrog Monster) ; corpus
  13833 → 13844 (+11), 0 perdue, 19 IR changées dont 0 déjà supportée.

## Étape 12 réalisée : entrée sur le champ de bataille comme motif de déplacement

- `enterBattlefield` (la source) et `permanentEntered` (un permanent
  observé) sont lus comme motifs `move` vers le champ de bataille, groupe
  `Entry`, par le correspondant unique de la famille changements de zone.
  Leurs attributs deviennent typés : origine (`enteredFrom`), état d'entrée
  (`entryState`), `nontoken`, type de créature choisi, pendant le combat,
  exclusion de la source, agrégation. Les tests écrits à la main du site
  d'entrée et leurs bras de `triggered_event_supported` sont retirés.
- Correction de règle assumée (CR 603.6a, regard en avant) : le prédicat
  d'un permanent qui entre est lu sur ses caractéristiques effectives sur le
  champ de bataille, plus sur sa carte imprimée (un terrain qui entre en
  étant une créature est une créature qui entre).
- Analyseur : `enters or dies` / `dies or enters` d'un même sujet est
  l'union des deux transitions (`oneOf`).
- Sondes : ligue et corpus inchangés (800, 13844), 0 perdue, 1 IR changée :
  les cartes `enters or dies` échouent ensuite dans leur corps.

## Familles suivantes

1. **Lancement (suite)** : prédicats numériques sur la pile, sorts qui
   ciblent, origine du sort.
2. **Marqueurs** : `countersPlaced`, `counterRemoved` → lemme `putCounters`.
3. **Entrées** : `enterBattlefield`, `permanentEntered` → lemme `move` vers le
   champ de bataille, avec l'état d'entrée.
4. **Agrégats** : `one or more` sur les dégâts (plusieurs sources), les
   départs et les sacrifices ; caractéristiques effectives dans l'instantané
   (sacrifice, prédicats numériques).
5. **Délais** : `at end of combat` (7 cartes `destroy that creature at end of
   combat` attendent ce déclencheur retardé).
