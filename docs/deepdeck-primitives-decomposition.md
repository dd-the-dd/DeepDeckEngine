# Inventaire du parseur promu : actions, contrôle et décompositions

Lecture seule du code actuel ; aucune modification de production et aucune compilation. Le relevé exhaustif est dans [INVENTORY.md](deepdeck-oracle-ir-nodes.md), avec noms, émetteurs et lignes. Les données structurées sont dans [classified-inventory.json](deepdeck-oracle-ir-classified.json).

## Ce que mesure cet inventaire

L'audit global fourni par root contient **17 771 capacités natives et 278 valeurs distinctes de `kind`**. Ce nombre inclut actions, coûts, expressions, références, événements, modificateurs, enveloppes et définitions imbriquées de jetons. Il ne représente pas 278 primitives d'action. Une occurrence dans une capacité n'est pas une preuve d'exécution.

Le compteur de 49 basé sur les préfixes n'est pas une frontière architecturale fiable : `castSpell` est ici une restriction de dépense de mana, `tap` un coût et `exile` une zone. À l'inverse, `modifyPowerToughness` peut être une action et un modificateur. Le classement manuel du fichier JSON retient une catégorie principale et conserve tous les rôles JSON observés ; ce classement n'est pas un nouveau compteur normatif de primitives.

| Catégorie principale | Kinds observés |
|---|---:|
| Actions de changement d'état/règles | 42 |
| Actions spécialisées/composites à examiner | 7 |
| Contrôle, mémoire, programmation différée | 14 |
| Choix et déclaration | 12 |
| Paiements/coûts | 7 |
| Signaux sémantiques | 2 |
| Expressions et prédicats | 54 |
| Références, collections, zones | 48 |
| Motifs d'événement | 19 |
| Règles continues/remplacements | 20 |
| Durée et timing | 6 |
| Descriptions de mana/restrictions | 5 |
| Enveloppes, mots-clés, autres métadonnées | 42 |
| **Total partitionné** | **278** |

Les constructeurs littéraux du code donnent 256 noms, dont six absents de cet audit. Inversement, 28 noms observés proviennent de constructeurs dynamiques, notamment les mots-clés. Ce rapprochement évite d'appeler du code « mort » sur la seule absence d'occurrence : `replaceTargetWithEach`, par exemple, est une représentation intermédiaire d'Overload consommée par la composition de carte.

## Actions réellement représentées

La liste exhaustive avec lignes est dans l'inventaire. Voici les principaux domaines à exposer à l'utilisateur :

| Domaine | Kinds d'action représentatifs |
|---|---|
| Vie, dommages, ressources | `gainLife`, `loseLife`, `dealDamage`, `addMana`, `addPlayerCounters`, `removePlayerCounters`, `putCounters` |
| Cartes et zones | `drawCards`, `discardCards`, `mill`, `lookAtTopCards`, `revealCards`, `revealHand`, `moveCards`, `shuffleZone`, `exileCards`, `exilePermanent`, `exileTargetCards`, `returnToOwnersHand`, `putPermanentOnOwnerLibrary`, `moveTriggeringCard` |
| Permanents et pile | `destroyPermanent`, `sacrificePermanent`, `sacrificePermanents`, `counterSpell`, `counterStackObject`, `untapPermanent`, `untapPermanents`, `transformPermanent` |
| Création et règles temporaires | `createTokens`, `createEmblem`, `becomeCreature`, `modifyPowerToughness`, `modifySubtypeSet`, `addSubtypeToPermanent`, `grantKeyword`, `grantAbility`, `installLoseAllAbilities`, `grantAdditionalLandPlays` |
| Préparation/entrée | `setEnteringState`, `putEnteringCounters`, `setPrepared` |

Le coût `tap` et le constructeur source `tapPermanent` ne doivent pas être confondus : le second est présent dans le code, mais absent de cet audit. De même, les paires coût/effet `discardCard`/`discardCards`, `exileCard`/`exileTargetCards`, `returnPermanentToOwnersHand`/`returnToOwnersHand` peuvent partager un service interne tout en conservant des contrats distincts : paiement exact et transactionnel, effet autant que possible, choix et légalité.

## Dix décompositions proposées, motivées par les corps

Les compositions ci-dessous sont des propositions de contrats partagés. Les briques manquantes doivent être prouvées avant de remplacer une opération actuelle.

| Priorité | Preuve actuelle | Décomposition proposée et précaution |
|---:|---|---|
| 1 | [counter_unless_paid.rs:93](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter_unless_paid.rs:93) émet `counterStackObjectUnlessPays` avec `spell` et `manaCost`. | Paiement optionnel par le contrôleur de l'objet de pile → résultat de paiement lié → `counterStackObject` si impayé. Partager la transaction de paiement ; un refus et une erreur sont différents. `counterSpell` et `counterStackObject` peuvent ensuite partager le service de contresort avec des sélecteurs typés. |
| 2 | [same_name_search.rs:69](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/same_name_search.rs:69) rassemble nom depuis cible, propriétaire, trois zones, choix, exil et mélange dans `exileNamedCardsFromZones`. | Lier propriétaire/nom avant déplacement → requête multi-zone → recherche/choix → exil par lot → mélange et signal de recherche. Préserver visibilité, choix facultatif d'échec de recherche et collection réellement déplacée ; un simple `forEach` n'est pas une équivalence. |
| 3 | [linked_entry_exile.rs:126](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/linked_entry_exile.rs:126) déclare déjà les cibles, puis `exilePermanentWithSource` répète candidats/min/max et cache `returnWhenSourceLeaves`. [activated_zone_change.rs:111](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/activated_zone_change.rs:111) émet aussi `exileTargetCardWithSource`. | Déplacement d'exil avec résultat d'objets réellement déplacés → association liée à l'incarnation de source → règle de durée/retour. La sortie puis le retour d'un même ID ne doit pas réutiliser l'ancienne identité. Conserver les admissions simultanées et remplacements. |
| 4 | [player_counter_sweep.rs:53](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/player_counter_sweep.rs:53) est déjà une suite ajout/choix/retrait de compteurs, mais termine par `destroyPermanentsByManaValue`. [collection_destroy.rs:8](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/collection_destroy.rs:8) dispose de `destroyPermanent` sur collection filtrée. | Requête partagée « valeur de mana ≤ résultat payé » → destruction de la collection. Il faut une liaison numérique dans le prédicat et la même sémantique de destruction par lot, sans valeur zéro de secours ni destruction sérielle inventée. |
| 5 | [draw_move.rs:46](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/draw_move.rs:46) exige exactement `[DrawCards, MoveCards]`, `Then`, même acteur, main→dessus de bibliothèque. Il émet pourtant déjà quatre actions séparées. [targetless.rs:283](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/targetless.rs:283) refuse `MoveCards` dans son abaissement ordinaire. | Rendre le déplacement sélectionné/ordonné un enfant ordinaire de `EffectProgram`, avec déclarations et liaisons lexicalement délimitées. Même axe pour [ability_loss_destroy.rs:31](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_loss_destroy.rs:31), qui attend une paire exacte. C'est une dette d'assemblage du parseur, pas un opcode combiné du moteur. Installer toutes les pertes d'abilités avant toute destruction reste essentiel. |
| 6 | [zone_change.rs:149](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/zone_change.rs:149) choisit différents opcodes selon destination/action ; [source_move.rs:13](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/source_move.rs:13) porte déjà propriétaire, destination, position et changement de face dans `moveCards`. | Réunir les adaptateurs retour/exil/carte/permanent autour du même service de déplacement avec résultat typé. Les variantes `exileAllGraveyards` et `exileTogether` sont des requêtes/politiques de lot à expliciter. **Détruire et sacrifier restent des actions sémantiques propres** : les remplacer par une écriture de zone casserait régénération, bouclier et décès. |
| 7 | [library_partition.rs:123](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/library_partition.rs:123), [conditional_zone_change.rs:77](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/conditional_zone_change.rs:77), [optional_zone_action.rs:151](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/optional_zone_action.rs:151), [event_bound_payment.rs:59](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/event_bound_payment.rs:59) produisent plusieurs dialectes de contrôle. | Uniformiser `conditional`/`conditionalEffect`, `optionalEffects`/`optionalAction` et paiement optionnel autour du programme typé partagé. Préserver acteur du choix, cibles à déclaration, paresse des branches, « if you do » et distinction refus/erreur. Ce sont des opérateurs de contrôle, pas des actions atomiques supplémentaires. |
| 8 | [equip_native.rs:31](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/equip_native.rs:31) décrit surtout `keywordAbility`, `equip` et coûts. [simple_keyword.rs:53](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:53) émet dynamiquement le nom canonique du mot-clé. | Des patrons déclaratifs peuvent développer les mots-clés composables : Equip = capacité activée/coût/cible/timing/attachement ; Prowess = événement/bonus/durée. Conserver identité et signal du mot-clé. Ne pas transformer sans preuve les mots-clés continus comme flying en fausses séquences d'actions. |
| 9 | [uncast_entry_replacement.rs:83](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/uncast_entry_replacement.rs:83) sélectionne `prohibitUncastBattlefieldEntry`, mais le modificateur inclut une destination d'exil. Son comportement ne se réduit donc pas au nom « interdire ». | Événement d'entrée proposé + prédicat de provenance du lancement + remplacement de destination, distinct d'une vraie interdiction d'entrée. Les politiques d'admission figées et l'ordre des remplacements doivent rester explicites ; exiler après entrée serait faux. |
| 10 | [temporary_source_animation.rs:99](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/temporary_source_animation.rs:99) fait porter à `becomeCreature` types/sous-types/couleurs/base P/T/conservation/durée, puis ajoute des effets séparés ; [graveyard_characteristic.rs:62](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/graveyard_characteristic.rs:62) dispose déjà de `setBasePowerToughness`. | Bundle d'effet continu partagé, composé de modificateurs de types, couleurs et P/T, avec **même identité/timestamp/durée**. Une découpe naïve en effets aux timestamps distincts changerait les couches et dépendances. |

## Décompositions déjà réussies à conserver

- [library_partition.rs:79](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/library_partition.rs:79) compile Scry/Surveil en observation, sélection, ordonnancement et déplacement avec le signal de l'abilité ; la destination distingue les deux procédures.
- [earthbend.rs:29](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/earthbend.rs:29) émet animation, mot-clé, compteurs, règle de changement de zone différé et signal.
- [amass.rs:37](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/amass.rs:37) utilise conditions, création éventuelle, sélection, compteurs, sous-type et signal, sans opcode moteur `amass`.
- [mobilize.rs:40](/D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/mobilize.rs:40) lie les jetons créés puis programme leur sacrifice différé. La liaison est aussi importante que l'action.
- `namedToken` est un descripteur de modèle de jeton dans `createTokens`, pas une fonction nommée d'après une carte.

## Frontières à rendre visibles dans le schéma

**Action** reçoit des opérandes typés ; **requête** décrit les objets ; **cardinalité** borne le choix ; **déclaration** choisit les cibles au bon moment ; **contrôle** compose les actions ; **durée** programme la vie des règles ; **signal** publie un événement sémantique. Les compteurs, types et noms ne doivent pas devenir de nouveaux opcodes.

`grantAdditionalLandPlays` a une durée de tour explicite, tandis que `additionalLandPlay` est un modificateur statique actif sur le champ de bataille : partager le service de quota est possible, mais les durées diffèrent. `setEnteringState` et `putEnteringCounters` interviennent avant l'événement d'entrée ; les remplacer par une action après entrée changerait les observations des déclencheurs.

Cet audit ne conclut ni à une accélération ni à un nombre de cartes débloquées. Il propose des frontières de décomposition. Le relevé moteur de root doit encore confirmer pour chaque opcode le comportement caché, les dépendances et les opérations réellement partageables.
