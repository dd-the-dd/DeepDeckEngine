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

## Suite

1. Cadre transmis aux corps modaux des déclencheurs ; relation de joueur
   d'un sort sur la pile ; `that player may` (permission facultative d'un
   autre joueur, moteur).
