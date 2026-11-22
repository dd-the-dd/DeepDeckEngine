# Qualificatifs d'objet (C4)

Ce document décrit comment les qualificatifs d'un nom d'objet (statuts,
propositions relatives, relations de contrôleur et de propriétaire, valeurs
choisies) se composent dans la requête d'objet partagée et dans les
conditions. Il suit la section C4 de l'audit de généralité.

## Principe

Un qualificatif est un prédicat d'objet ordinaire. Le même prédicat est lu
par les sélecteurs (`target enchanted permanent`, `each modified creature`),
par les conditions (`if it was kicked`, `as long as this creature is
equipped`) et par les comptes (`for each modified creature you control`).
Aucun lecteur d'action ne connaît ces mots.

## Registre des prédicats de statut (`automata/designation.rs`)

`lookup_status_predicate` est l'unique point d'entrée des adjectifs de
statut :

| Adjectif | Prédicat | IR |
|---|---|---|
| `tapped`, `untapped`, `attacking`, `blocking` | `ObjectStatus` | `isTapped`, `not isTapped`, `isAttacking`, `isBlocking` |
| `modified` (CR 700.9) | `ObjectStatus::Modified` | `isModified` |
| `enchanted` (CR 303.4) | `ObjectStatus::Enchanted` | `hasAttachedAura` |
| `equipped` (CR 301.5) | `ObjectStatus::Equipped` | `isEquipped` |
| `monstrous`, `renowned`, `suspected` | `PermanentDesignation` | `hasDesignation` |
| `kicked` (CR 702.33d) | `PermanentDesignation::Kicked` | `hasDesignation` (`kicked`) |
| `face-down` (CR 708.2) | `PermanentDesignation::FaceDown` | `hasDesignation` (`faceDown`) |

- Sélecteur : l'atome de prédicat lit le registre. `equipped creature` ou
  `enchanted permanent` **sans déterminant** devant et suivi d'un nom au
  singulier désigne l'objet auquel la source est attachée (CR 301.5c,
  303.4e) : l'atome refuse alors le mot et la référence d'attachement le
  lit, comme avant. Après un déterminant (`target`, `an`, `each`…) ou
  devant un pluriel, c'est le qualificatif.
- Condition d'état de la source : tout statut du registre sans proposition
  propre devient `objectMatchesFilter` sur la source ; une liste
  coordonnée (`enchanted or equipped`) est l'union (ou l'intersection) de
  ses statuts ; un seul connecteur par liste.
- Moteur : `hasDesignation` accepte `kicked` (drapeau posé à l'entrée d'un
  permanent dont le sort a été kické) et `faceDown`.

Preuves : `status_adjectives_come_from_one_registry_for_selectors_and_conditions`
(analyse) et `status_registry_atoms_read_kicked_equipped_and_enchanted_permanents`
(moteur).
