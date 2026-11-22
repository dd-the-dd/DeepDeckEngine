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

## Proposition relative (`automata/relative_clause.rs`)

La requête d'objet partagée a un emplacement de proposition relative,
lu après les qualificatifs postnominaux (`with …`, `you control`) :

```text
Relative := ("that" | "which" | "who") Clause
Clause   := ("has" | "have") CounterQualifier                 présent
          | ("is" | "are" | "isn't" | "aren't") ["not"] Statut ("or" Statut)*
          | Passé ("or" Passé)* "this" "turn"                  historique
Passé    := "entered" ["the" "battlefield"] | "attacked" | "blocked"
          | ("was" | "were") "dealt" "damage" | "dealt" "damage"
          | ("didn't" | "did" "not") Verbe                     négation distribuée
```

- Le présent réutilise les enfants existants : le qualificatif de marqueurs
  et le registre de statuts.
- Le passé avec `this turn` nomme un fait de l'objet pendant ce tour
  (`enteredThisTurn`, `attackedThisTurn`, `blockedThisTurn`,
  `wasDealtDamageThisTurn`, `dealtDamageThisTurn`) ; `didn't attack or
  enter` est l'intersection des négations.
- Les autres relatives (`that player controls`, `that targets …`) ne sont
  pas revendiquées ici et gardent leur échec explicite.

Moteur : l'historique est lu dans le journal d'événements du tour, pour la
même incarnation de l'objet (`objectIdentity` de l'événement) ; aucun
nouveau journal n'est ajouté (`attackerDeclared`, `blockerDeclared`,
`damageDealt` et le drapeau `enteredThisTurn` existent). Le filtre effectif
et le nouveau `permanent_state_matches` (qui remplace l'appel au filtre
d'état sans journal dans les comptes, conditions de présence et filtres
d'événement) le lisent. Un sélecteur statique, évalué sans journal, refuse
ces atomes (`filter_reads_object_history`) au lieu de les lire faux.

Preuves : `relative_clauses_fill_the_shared_object_query_slot` (analyse,
sœur de compte, quasi-échecs) et
`relative_history_clauses_read_this_turns_events_for_the_same_object`
(moteur : compte, `destroy all`, cible).
