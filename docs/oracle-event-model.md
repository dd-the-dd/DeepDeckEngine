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

## Familles suivantes

1. **Lancement** : `spellCast` et ses ordinaux (`spellCastOrdinal`) → lemme
   `cast` ; l'ordinal par lanceur et par tour est déjà émis, il reste à le
   lire comme modificateur du motif générique.
2. **Marqueurs** : `countersPlaced`, `counterRemoved` → lemme `putCounters`.
3. **Entrées** : `enterBattlefield`, `permanentEntered` → lemme `move` vers le
   champ de bataille, avec l'état d'entrée.
4. **Agrégats** : `one or more` sur les dégâts (plusieurs sources), les
   départs et les sacrifices ; caractéristiques effectives dans l'instantané
   (sacrifice, prédicats numériques).
5. **Délais** : `at end of combat` (7 cartes `destroy that creature at end of
   combat` attendent ce déclencheur retardé).
