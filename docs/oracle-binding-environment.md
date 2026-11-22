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

## Suite

1. `it` sujet d'une action désignant l'objet qui est le sujet de
   l'événement (`Whenever a creature you control attacks, it gets +2/+2`).
2. Génitif numérique (`that creature's power`, `the sacrificed creature's
   toughness`) par le même résolveur, à la place de `numeric.event-anaphor`.
3. Cartes ayant changé de zone (`return it to its owner's hand` après une
   mort, CR 400.7) et références liées (CR 607, `the exiled card`).
