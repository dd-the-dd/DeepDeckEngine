# Environnement de liaison des références (capacité C2, 22 novembre 2026)

Complète `oracle-compositional-compiler.md` (domaine « Référence »),
`oracle-clause-layer.md` et `oracle-event-model.md`.

## Diagnostic

Les références (`it`, `that creature`, `that player`, `defending player`,
`its controller`, `that NOUN's owner`) étaient résolues par des lecteurs
locaux, chacun avec son propre paramètre d'antécédent : `event_player`,
`defending_player`, `combat_counterpart` (booléens du contexte), une
`PlayerBinding` passée au lecteur de sujet de joueur, un
`ObjectSubjectAntecedent` passé au lecteur de relation, un `TargetBinding`
passé à `reference.rs`, `DamageSourceAntecedent` pour la source des dégâts.
Un enfant sans paramètre (le destinataire des dégâts) ne voyait aucune
liaison : `deals 2 damage to that player` échouait en `target-list.slot.start`
(128 cartes du corpus à un seul blocage).

## Cible

```text
Environnement := Cadre (publié par l'événement de la capacité)
               + Lexical (position courante dans le programme)
Cadre    := joueur d'événement | joueur défenseur | autre combattant
            | objet d'événement (étape suivante) | quantité d'événement
Lexical  := objet singulier précédent (+ sélecteur déclaré) | joueur déclaré
Référence := `that player` | `defending player` | `it` | `that NOUN`
           | GÉNITIF ; GÉNITIF := Référence `'s` (controller | owner | ...)
```

- `automata/binding_environment.rs` : `FrameBindings`, `BindingEnvironment`,
  `BoundPlayerReference` et le résolveur unique (`recognize_bound_player_reference`,
  `recognize_bound_object_reference`, `recognize_that_player`,
  `recognize_defending_player`, `recognize_object_related_player`,
  `recognize_nominal_object_reference`).
- Transport : le parent qui possède la position installe l'environnement sur
  le curseur (`replace_binding_environment`) pour les enfants qu'il appelle,
  comme la quantité d'événement (`that many`). La séquence d'effets en
  installe un par action et restaure l'environnement englobant à sa sortie.
  Un parent qui n'en installe pas ne lie rien : toute référence y échoue
  explicitement.
- Les anciens lecteurs (`player_subject.rs`, anaphores de
  `permanent_subject.rs`) délèguent désormais au résolveur ; leurs états de
  diagnostic sont conservés.
- Abaissement : `bound_player_reference_value` / `bound_permanent_reference_value`
  (`effects/permanent_subject.rs`) donnent l'IR d'une référence à partir des
  emplacements déclarés du programme ; une cible absente reste une erreur.
- Moteur : les références émises sont des références existantes
  (`triggeringPlayer`, `chosenTarget`, `controllerOf`/`ownerOf` avec
  information de dernier état des cibles, `combatCounterpart`,
  `attachedPermanent`), plus `observedEventObject` comme référence d'objet
  (incarnation observée à l'entrée seulement, CR 400.7).

## Étape 1 réalisée : destinataires de dégâts liés, joueur d'événement

- Le destinataire des dégâts lit `that player`, `defending player`,
  `its controller`, `that NOUN's controller|owner`, `it` et `that NOUN` dans
  l'environnement (`DamageRecipientNode::BoundPlayer` / `BoundObject`).
- Le joueur qui pioche (CR 121.1) et le lanceur d'un sort (CR 601.2) sont
  des joueurs d'événement : leurs enregistrements lient déjà
  `triggeringPlayerId`. Une copie (`cast or copy`) et `cast this spell` en
  sont exclus.
- Sondes (base 08386d1) : ligue 800 → 800 (0 gagnée, 0 perdue, 1 IR changée,
  non supportée avant ni après) ; corpus 13851 → 13920 (+69 : Underworld
  Dreams, Megrim, Consign to the Pit, Eidolon of the Great Revel, Ornery
  Goblin, …), 0 perdue, 81 IR changées dont 0 déjà supportée.
- Restent explicites : la relation de joueur d'un objet d'événement
  (`that land's controller` après `Whenever a land enters`, la créature qui
  meurt), `that player` désignant le contrôleur d'un objet d'événement
  (`Whenever a creature an opponent controls dies, that player ...`).

## Étape 2 réalisée : objet d'événement

- Le cadre publie l'objet que l'enregistrement lie (`EventObjectSlot`) avec
  le prédicat écrit par l'événement : le permanent qui entre (référence
  observée existante), et `TypedObservedEventObject::EventPermanent` (IR
  `triggeringPermanent`) pour le permanent qui meurt, l'attaquant, le
  permanent qui subit les dégâts, la source qui inflige des dégâts de combat
  à un joueur. `that NOUN` et `that NOUN's controller|owner` le désignent
  quand aucun antécédent lexical n'a ce nominal et que le prédicat de
  l'événement l'implique.
- Moteur (`engine/event_object.rs`) : à la mise sur la pile, l'incarnation
  de `triggeringPermanentId` est capturée avec son contrôleur et son
  propriétaire ; `triggeringPermanent` ne désigne l'objet que s'il est la
  même incarnation (CR 400.7) ; la sortie centrale du champ de bataille
  rafraîchit le contrôleur et le propriétaire en information de dernier
  état (CR 608.2h) ; `controllerOf`/`ownerOf` d'un objet d'événement lit
  l'objet vivant, sinon cette information.
- Sondes (base de l'étape 1) : ligue inchangée (800, 0 IR changée) ; corpus
  13920 → 13949 (+29 : Stinkweed Imp, Serra Redeemer, Dingus Staff, Zo-Zu
  the Punisher, Anointer of Valor, …), 0 perdue, 38 IR changées dont 0 déjà
  supportée.

## Étape 3 réalisée : `it` sujet de l'événement

- `EventObjectSlot::subject` : l'objet est le sujet grammatical de
  l'événement (attaquant, permanent qui meurt ou entre, source de dégâts de
  combat à un joueur). Sans liaison de source, le pronom `it` / `its` le
  désigne (`Whenever a creature you control attacks, it gets +2/+2`,
  `Whenever a creature dies, its controller ...`). Le permanent qui subit
  des dégâts n'est nommé que par son nominal (`that creature`).
- Sondes (base de l'étape 2) : ligue 800 → 801 (+1 : Dreadhorde Invasion),
  0 perdue ; corpus 13949 → 13965 (+16 : Fervent Charge, Glory Bearers,
  Atarka, Bereavement, Stonehoof Chieftain, …), 0 perdue, 31 IR changées
  dont 0 déjà supportée.

## Étape 4 réalisée : génitif numérique

- `that NOUN's power|toughness|mana value` est une primaire de la grammaire
  arithmétique partagée et l'objet de `equal to` sans cible fournie par le
  parent, résolue par l'environnement : une cible lexicale
  avec ce nominal garde la propriété de cible existante
  (`QualifiedTargetProperty`, information de dernier état des cibles) ;
  sinon l'objet d'événement (`NumericExpression::BoundObjectProperty`, IR
  `powerOf|toughnessOf|manaValueOf` de `triggeringPermanent` ou
  `observedEventObject`) ; `that spell's mana value` après un lancement lit
  l'enregistrement de lancement (`triggeringSpellManaValue`).
- Moteur : la force, l'endurance et la valeur de mana de l'objet
  d'événement sont lues vivantes tant que l'incarnation est la même, sinon
  en information de dernier état (capturée à la mise sur la pile,
  rafraîchie à la sortie du champ de bataille).
- Sondes (base de l'étape 3) : ligue 801 → 802 (+1 : Erratic Cyclops),
  0 perdue ; corpus 13965 → 13983 (+18 : Orchard Warden, Kresh the
  Bloodbraided, Hamletback Goliath, Manaplasm, Kaervek the Merciless, Heal
  the Scars, …), 0 perdue, 27 IR changées dont 0 déjà supportée.
- Reste explicite : `where X is that creature's mana value` après une cible
  détruite (Death Mutation : `lower.where-x.definition` ne représente pas la
  propriété de cible), `that card's` (cartes révélées), `that spell's` hors
  valeur de mana.

## Étape 5 réalisée : carte qui a changé de zone, exil lié (CR 607)

- Cadre : `FrameBindings::event_card` est la zone publique où le changement
  de zone déclencheur a mis son objet (mort, mise au cimetière depuis le
  champ de bataille, exil depuis le champ de bataille de la source). Le
  parent de déclencheur l'installe aussi pour l'abaissement ; sans
  collection observée du programme, `it` / `that card` d'une action sur
  carte est `{"kind":"triggeringCard"}`.
- Moteur : à la mise sur la pile, si l'objet n'est plus sur le champ de
  bataille, sa zone publique et l'identité de la carte qu'il y est devenu
  sont capturées ; `triggeringCard` ne la trouve que tant qu'elle reste cet
  objet (CR 400.7). Une main, une bibliothèque ou un cimetière sont toujours
  ceux du propriétaire (CR 400.3) : `its owner's hand|library|graveyard`
  est accepté comme destination par propriétaire.
- Exil lié : `the exiled card(s)` reprend l'exil du programme, sinon, comme
  `[each|all] card(s) exiled with this NOUN|it`, désigne l'ensemble lié
  `{"kind":"cardsExiledWithSource","link":"face"}`. La passe de composition
  existante (`card_composition.rs`) relie désormais au lieu de rejeter :
  chaque `exilePermanent` des autres capacités de la face reçoit
  `linkToSource` et le moteur enregistre le lien à la résolution ; un autre
  exil reste rejeté (`composition.linked-exile.relation`) ; un lecteur sans
  producteur sur sa face échoue en `binding.linked-exile.producer`.
- Sondes (base de la phase 2, 207a2e1 : ligue 802, corpus 14096) : ligue
  802 → 807 (+5 : Rancor, Oblivion Ring, Fake Your Own Death, False Demise,
  Malakir Rebirth), 0 perdue ; corpus 14096 → 14186 (+90 : Endless
  Cockroaches, Fiend Hunter, Demonic Vigor, The Cauldron of Eternity,
  Undying Rage, …), 0 perdue, 128 IR changées dont 0 déjà supportée.

## Étape 6 réalisée : lecteurs hérités migrés, cadre unique

- Le contexte de reconnaissance ne porte plus six drapeaux d'événement
  (`event_player`, `defending_player`, `combat_counterpart`, `event_object`,
  `event_spell`, `event_card`) mais un seul `frame: FrameBindings`, que le
  parent de déclencheur remplit et que tous les lecteurs lisent ; le
  contexte de sujet des propositions (`EffectSubjectContext`) le reprend.
- `LocalPlayerReference::Bound` : sans liaison de cible propre au lecteur,
  `that player` de `reference.rs` est résolu par l'environnement ; il est
  abaissé sans déclaration (`triggeringPlayer`, relation d'un objet
  d'événement) ou échoue explicitement.
- `DamageSourceAntecedent` est supprimé : le sujet de la proposition de
  dégâts est un emplacement de l'environnement (`subject`, installé par
  `with_clause_subject` autour des arguments), dérivé de l'opérande source
  (`damage_source_subject`) avec l'indice de l'action courante.
- Refactorisation : ligue et corpus inchangés (807, 14186), 0 IR changée.
  Les 23 cartes `that player may ...` atteignent désormais leur vrai
  blocage (le lecteur facultatif d'un autre joueur, `effect.library-search.action`).

## Étape 7 réalisée : emplacements élargis

- La cible unique protégée par une prévention est l'antécédent suivant ;
  piocher, meuler ou défausser ne déplacent aucun permanent et conservent
  l'antécédent de permanent (une cible choisie avant reste `that creature`).
- `EventObjectSlot::reference` est une référence de permanent quelconque :
  `the upkeep of enchanted NOUN's controller` publie le permanent enchanté
  avec le nominal écrit (`StepPlayerScope::AttachedPermanentController`).
- `counter that spell` après un lancement contrecarre le sort que
  l'enregistrement de lancement lie (`triggeringStackObject`).
- `where X is that NOUN's PROPERTY` lit la propriété de la cible déclarée
  (information de dernier état une fois détruite).
- Sondes (base de l'étape 6) : ligue inchangée (807) ; corpus 14186 →
  14198 (+12 : Death Mutation, Artifact Mutation, Unstable Mutation, Grave
  Strength, Hoard-Smelter Dragon, …), 0 perdue, 14 IR changées dont 0 déjà
  supportée.
- Restent explicites : `that spell's controller` (relation d'un sort sur la
  pile), le corps modal d'un déclencheur (Pip-Boy 3000 : le mode ne reçoit
  pas le cadre), `that player may ...` (lecteur facultatif d'un autre joueur).

## Étape 8 réalisée : appariement des capacités liées (CR 607.2a)

- Un lecteur est apparié à l'exil de sa propre capacité, sinon aux autres
  capacités de la face qui exilent des permanents. `cards exiled with this
  NOUN|it` (marqueur `link: face`) lit tous les exils de l'objet, comme le
  prévoit CR 607.2a (Helvault). `the exiled card(s)` (marqueur `link:
  pair`) désigne l'unique exil apparié : deux capacités candidates rendent
  l'appariement ambigu et le lecteur échoue en
  `binding.linked-exile.ambiguous`.
- Sondes (base de la phase 3, f007360 : ligue 808, corpus 14243) : ligue et
  corpus inchangés, 0 perdue. IR changée de 8 cartes déjà supportées
  (Oblivion Ring, Journey to Nowhere, Fiend Hunter, …) : seul le marqueur du
  lecteur passe de `face` à `pair`, le comportement est identique.

## Étape 9 réalisée : corps modaux des déclencheurs

- Les modes d'un déclencheur modal lisent le cadre de son événement (objet
  observé, attachement de la source, `FrameBindings`) et sa quantité
  d'événement (`that many`) ; leurs cadres de phrase restent ceux de la
  levée modale partagée (`recognize_effect_sequence_modal_with_context`).
- Sondes (base de l'étape 8) : ligue inchangée (808) ; corpus 14243 →
  14250 (+7 : Pip-Boy 3000, Hivespine Wolverine, Tiller Engine, …), 0
  perdue, 9 IR changées dont 0 déjà supportée.

## Étape 10 réalisée : cartes meulées comme collection observée

- Une meule dont l'action suivante lit une collection de cartes (`from
  among them`, `that card`, directement ou en tête d'une séquence
  facultative) lie les cartes meulées (`bind`, primitive existante) et les
  rend collection observée du programme (zone : cimetière).
- Sondes (base de l'étape 9) : ligue inchangée (808) ; corpus 14250 →
  14259 (+9 : Something Worth Saving, Midnight Tilling, Vastlands
  Scavenger, Sludge Titan, …), 0 perdue, 15 IR changées dont 0 déjà
  supportée.
- Reste explicite : `reveal … until` dans une branche `If you do` (Foster),
  le génitif sur une carte observée (`that card's mana value`, 29 cartes :
  l'abaissement numérique n'a pas accès à la collection observée).

## Étape 11 réalisée : antécédent de la condition, sujets bloqué ou blessé, contrôleur du sort

- Une condition intercalée qui teste un seul permanent (`if this permanent
  is an enchantment`) en fait l'antécédent lexical du corps (`it becomes`).
- L'attaquant bloqué (`becomes blocked`) et le permanent blessé (`is dealt
  damage`) sont des objets d'événement sujets : `it` les désigne.
- `that spell's controller` après un lancement : le contrôleur du sort que
  l'enregistrement lie (`controllerOf triggeringStackObject`) ; une fois le
  sort parti de la pile, son lanceur (CR 110.2). Un changement de
  contrôle d'un sort sur la pile après le déclenchement n'est pas suivi
  comme information de dernier état (limite documentée).
- Sondes (base de l'étape 10) : ligue inchangée (808) ; corpus 14259 →
  14275 (+16 : Opal Champion et la famille Opal/Hidden, Mana Vault,
  Tattered Ratter, Somberwald Alpha, …), 0 perdue, 28 IR changées dont 0
  déjà supportée.
- Reste explicite : `that player's graveyard` (détenteur de zone lié), `it`
  désignant une cible de carte mise sur le champ de bataille (`It gains
  haste`), `it` source de dégâts observée (Wrathful Red Dragon).

## Étape 12 réalisée : permission d'un autre joueur

- `that player may …`, `defending player may …` et `its controller may …`
  ouvrent le même programme facultatif que `you may` ; l'acteur est lu dans
  l'environnement du cadre (joueur de l'événement, contrôleur de l'objet
  observé) et abaissé par `bound_player_reference_value`.
- Le connecteur de résultat est généralisé : `if/when they do/don't`,
  `if the player does/doesn't`, `if that player does/doesn't` désignent
  l'exécutant de la permission (CR 608.2c).
- Le moteur possédait déjà la primitive : `optionalEffects` et
  `optionalPayCostPerformEffects` adressent la décision au joueur résolu ;
  aucun nouveau type d'action. Test d'exécution : la décision est demandée au
  joueur de l'événement, pas au contrôleur.
- Sonde : ligue 808 → 808, corpus 14275 → 14275, 0 perte, 1 IR modifiée
  (non supportée, bloqueur suivant). Les cartes `that player may` atteignent
  désormais leurs bloqueurs suivants (`they draw`, `the creature`,
  `choose any number`).

## Étape 13 (lot architecture S2) : antécédent ActionResult, nombre antérieur

Réalisée sans compilation dans le lot S2 ; preuve à l'intégration.

- `BindingEnvironment::action_result` (`ActionResultSlot { performer }`) :
  l'antécédent qu'un connecteur de résultat (`If you do`, `If they don't`,
  `When that player does`) observe. Le résolveur unique
  `recognize_outcome_connector` vérifie que le sujet du connecteur nomme
  l'exécutant du producteur (`you may` → `you` ; `that player may` →
  `they` / `that player` / `the player` ; instruction obligatoire → tout
  sujet) et échoue sinon en `binding.action-result.performer`.
- `CostOutcomeContinuation` et la grammaire du connecteur quittent
  `resolution_cost.rs` : `PerformanceOutcome { Performed, Declined,
  Reflexive }`. Les trois nœuds `OptionalCost`, `OptionalAction`,
  `ReflexiveAction` deviennent un seul `EffectProgramNode::Outcome
  { permission, producer: Cost | Action, outcome, body }`.
- Nouveau : `ACTION. If you do, BODY.` après une instruction obligatoire
  s'abaisse en `performAction{action, onPerformed: BODY}` (primitive
  existante, témoin mesuré). `If you don't` après une instruction
  obligatoire reste un échec explicite (aucune branche déclinée au moteur).
- `BindingEnvironment::number` : `that number` est une primaire de la
  grammaire arithmétique partagée (`with_prior_number`). Retire
  `recognize_that_number_plus_literal` ; `half` lit le compte de la
  bibliothèque comme opérande ordinaire et retire
  `recognize_half_controller_library_count` (backlog #14).
- `possessive_nominal` et `object_related_player_head` sont le seul lecteur
  lexical du génitif ; `typed_target_reference.rs` les appelle.
- Moteur : `target_numeric_lki.rs` devient `object_lki.rs` (instantané
  d'identité : valeur de mana, force, endurance, contrôleur, propriétaire).

## Suite

0. Restes du lot S2 : promotions `optional_bound_mill`,
   `landfall_reflexive_counter`, `overload` et `DestroyReferencedSet`
   (anaphore d'ensemble encore passée en paramètre par `recognize_effect`,
   propriété de S3) ; épissage JSON `replace_x_value` (where-X) ; anaphores
   locales de `conditional.rs`, `damage_replacement.rs`,
   `sentence_condition.rs`, `permanent_subject.rs`, `action_result.rs` ;
   transport `event_numeric_binding` du curseur à replier dans le cadre.
1. Collections révélées ou regardées (`reveal … until` dans une branche
   `If you do`) et génitif numérique d'une carte observée
   (`that card's mana value`, environ 29 cartes) : l'abaissement numérique
   n'a pas encore accès aux collections.
2. `that player's graveyard` (détenteur de zone lié dans la requête) et `it`
   pour la carte déplacée par un changement de zone ciblé.
3. Source de dégâts observée (`it`), retard avec jeton ou cible
   (`Sacrifice it at the beginning of the next end step`), changement de
   contrôle d'un sort (LKI non suivie, limite documentée).
