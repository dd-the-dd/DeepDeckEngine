# Inventaire des kinds réellement observés et de leurs émetteurs

Source : `.tmp\night-reflecting-public-root-01\global-after.jsonl` ; 17771 capacités natives,278 valeurs distinctes de kind. Ces valeurs ne sont pas278 actions ni278 primitives.

Présence dans une IR ne prouve pas qu’une action a été exécutée. Les occurrences dans les définitions de jetons/abilités sont conservées. Une capacité contenant plusieurs occurrences compte une seule fois dans nativeAbilityCount.

## Actions de changement d’état/règles

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `addMana` | 1836 | [src/oracle/canonical/abilities/restricted_mana_native.rs:71](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/restricted_mana_native.rs:71) | effects, then |
| `addPlayerCounters` | 1 | [src/oracle/canonical/effects/player_counter_sweep.rs:53](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/player_counter_sweep.rs:53) | effects |
| `addSubtypeToPermanent` | 2 | [src/oracle/canonical/effects/amass.rs:70](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/amass.rs:70) | effects |
| `becomeCreature` | 13 | [src/oracle/canonical/effects/earthbend.rs:31](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/earthbend.rs:31) | effects |
| `counterSpell` | 92 | [src/oracle/canonical/effects/counter.rs:132](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter.rs:132) | effects |
| `counterStackObject` | 4 | [src/oracle/canonical/effects/counter.rs:161](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter.rs:161) | effects, then |
| `createEmblem` | 1 | [src/oracle/canonical/effects/emblem.rs:40](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/emblem.rs:40) | effects |
| `createTokens` | 213 | [src/oracle/canonical/abilities/saga_native.rs:78](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:78) | effects, declineEffects, then |
| `dealDamage` | 594 | [src/oracle/canonical/effects/damage.rs:168](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/damage.rs:168) | effects, then, else |
| `destroyPermanent` | 573 | [src/oracle/canonical/effects/ability_loss_destroy.rs:86](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_loss_destroy.rs:86) | effects, then, else |
| `discardCards` | 178 | [src/oracle/canonical/effects/hand_disruption.rs:86](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/hand_disruption.rs:86) | effects |
| `drawCards` | 1046 | [src/oracle/canonical/abilities/triggered_library.rs:91](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_library.rs:91) | effects, then, declineEffects |
| `exileCards` | 1 | [src/oracle/canonical/effects/optional_zone_action.rs:154](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/optional_zone_action.rs:154) | action |
| `exilePermanent` | 106 | [src/oracle/canonical/effects/delayed_blink.rs:126](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_blink.rs:126) | effects, then |
| `exileTargetCards` | 46 | [src/oracle/canonical/effects/activated_zone_change.rs:118](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/activated_zone_change.rs:118) | effects, then |
| `gainLife` | 542 | [src/oracle/canonical/abilities/blight_native.rs:142](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/blight_native.rs:142) | effects, then |
| `grantAbility` | 3 | [src/oracle/canonical/abilities/saga_native.rs:154](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:154) | effects |
| `grantAdditionalLandPlays` | 4 | [src/oracle/canonical/effects/player_permission.rs:23](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/player_permission.rs:23) | effects |
| `grantKeyword` | 377 | [src/oracle/canonical/effects/attached_keyword_grant.rs:35](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/attached_keyword_grant.rs:35) | effects, else, then, modifiers |
| `installLoseAllAbilities` | 1 | [src/oracle/canonical/effects/ability_loss_destroy.rs:80](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_loss_destroy.rs:80) | effects |
| `lookAtTopCards` | 261 | [src/oracle/canonical/effects/activated_top_card_delayed_draw.rs:99](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/activated_top_card_delayed_draw.rs:99) | effects, then |
| `loseLife` | 209 | [src/oracle/canonical/effects/damage.rs:368](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/damage.rs:368) | effects |
| `mill` | 2 | [src/oracle/canonical/abilities/triggered_attack_mill.rs:93](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_attack_mill.rs:93) | action, effects |
| `modifyPowerToughness` | 494 | [src/oracle/canonical/abilities/saga_native.rs:61](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:61) | effects, else, then, modifiers |
| `modifySubtypeSet` | 1 | [src/oracle/canonical/effects/temporary_source_animation.rs:134](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/temporary_source_animation.rs:134) | effects |
| `moveCards` | 651 | [src/oracle/canonical/effects/card_type_partition.rs:57](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/card_type_partition.rs:57) | effects, then, onPerformed |
| `moveTriggeringCard` | 2 | [src/oracle/canonical/effects/earthbend.rs:58](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/earthbend.rs:58) | effects |
| `putCounters` | 469 | [src/oracle/canonical/abilities/blight_native.rs:73](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/blight_native.rs:73) | effects, then |
| `putEnteringCounters` | 1 | [src/oracle/canonical/effects/delve.rs:64](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delve.rs:64) | replacement |
| `putPermanentOnOwnerLibrary` | 1 | [src/oracle/canonical/effects/owner_library_reveal.rs:90](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/owner_library_reveal.rs:90) | effects |
| `removePlayerCounters` | 1 | [src/oracle/canonical/effects/player_counter_sweep.rs:70](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/player_counter_sweep.rs:70) | effects |
| `returnToOwnersHand` | 142 | [src/oracle/canonical/effects/zone_change.rs:158](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/zone_change.rs:158) | effects, then, else |
| `revealCards` | 124 | [src/oracle/canonical/effects/card_type_partition.rs:95](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/card_type_partition.rs:95) | effects, then |
| `revealHand` | 18 | [src/oracle/canonical/effects/hand_disruption.rs:69](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/hand_disruption.rs:69) | effects |
| `sacrificePermanent` | 713 | [src/oracle/canonical/abilities/activated_native.rs:120](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:120) | costs, additionalCosts, effects |
| `sacrificePermanents` | 6 | [src/oracle/canonical/effects/mobilize.rs:62](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/mobilize.rs:62) | effects |
| `setBasicLandType` | 1 | [src/oracle/canonical/effects/chosen_land_type.rs:123](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_land_type.rs:123) | replacement |
| `setEnteringState` | 644 | [src/oracle/canonical/effects/chosen_land_type.rs:131](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_land_type.rs:131) | replacement, then |
| `setPrepared` | 42 | [src/oracle/canonical/effects/optional_zone_action.rs:126](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/optional_zone_action.rs:126) | replacement, onPerformed |
| `shuffleZone` | 271 | [src/oracle/canonical/effects/library_search_move.rs:87](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/library_search_move.rs:87) | effects, then |
| `transformPermanent` | 24 | [src/oracle/canonical/effects/targetless.rs:246](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/targetless.rs:246) | effects |
| `untapPermanent` | 14 | [src/oracle/canonical/effects/effect_sequence.rs:404](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/effect_sequence.rs:404) | effects |
| `untapPermanents` | 1 | [src/oracle/canonical/abilities/activated_library_search_native.rs:69](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_library_search_native.rs:69) | then |

## Actions spécialisées ou composites à examiner

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `counterStackObjectUnlessPays` | 39 | [src/oracle/canonical/effects/counter_unless_paid.rs:93](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter_unless_paid.rs:93) | effects |
| `destroyPermanentsByManaValue` | 1 | [src/oracle/canonical/effects/player_counter_sweep.rs:76](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/player_counter_sweep.rs:76) | effects |
| `exileAllGraveyards` | 4 | [src/oracle/canonical/effects/graveyard_destination.rs:40](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/graveyard_destination.rs:40) | effects |
| `exileNamedCardsFromZones` | 2 | [src/oracle/canonical/effects/same_name_search.rs:69](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/same_name_search.rs:69) | effects |
| `exilePermanentWithSource` | 56 | [src/oracle/canonical/effects/linked_entry_exile.rs:137](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/linked_entry_exile.rs:137) | effects |
| `exileTargetCardWithSource` | 21 | [src/oracle/canonical/effects/activated_zone_change.rs:111](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/activated_zone_change.rs:111) | effects |
| `exileTogether` | 1 | [src/oracle/canonical/effects/exile.rs:98](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/exile.rs:98) | effects |

## Contrôle, mémoire et programmation différée

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `bind` | 30 | [src/oracle/canonical/effects/ability_loss_destroy.rs:68](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_loss_destroy.rs:68) | effects, then |
| `chooseOneEffect` | 2 | [src/oracle/canonical/effects/named_token_creation.rs:50](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/named_token_creation.rs:50) | effects |
| `conditional` | 81 | [src/oracle/canonical/abilities/additional_cost_native.rs:122](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/additional_cost_native.rs:122) | replacement, effects, then, additionalCosts |
| `conditionalEffect` | 22 | [src/oracle/canonical/abilities/activated_library_search_native.rs:66](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_library_search_native.rs:66) | effects, then |
| `createReflexiveTrigger` | 1 | [src/oracle/canonical/effects/landfall_reflexive_counter.rs:147](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/landfall_reflexive_counter.rs:147) | then |
| `forEachObject` | 3 | [src/oracle/canonical/effects/ability_loss_destroy.rs:76](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_loss_destroy.rs:76) | then, effects |
| `forEachPlayer` | 25 | [src/oracle/canonical/effects/quantified_player_action.rs:49](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/quantified_player_action.rs:49) | effects |
| `installDelayedStepTrigger` | 10 | [src/oracle/canonical/effects/activated_top_card_delayed_draw.rs:105](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/activated_top_card_delayed_draw.rs:105) | effects, then |
| `installDelayedTriggeredAbility` | 2 | [src/oracle/canonical/effects/protected_attack_trigger.rs:55](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/protected_attack_trigger.rs:55) | effects |
| `installDelayedZoneChangeTrigger` | 2 | [src/oracle/canonical/effects/earthbend.rs:53](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/earthbend.rs:53) | effects |
| `optionalAction` | 2 | [src/oracle/canonical/effects/optional_bound_mill.rs:75](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/optional_bound_mill.rs:75) | effects |
| `optionalEffects` | 246 | [src/oracle/canonical/effects/delayed_draw.rs:82](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_draw.rs:82) | effects, declineEffects |
| `optionalPayCostPerformEffects` | 4 | [src/oracle/canonical/effects/event_bound_payment.rs:59](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/event_bound_payment.rs:59) | effects |
| `storeDecision` | 56 | [src/oracle/canonical/effects/chosen_card_name.rs:138](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:138) | replacement |

## Choix et déclaration

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `chooseBasicLandType` | 1 | [src/oracle/canonical/effects/chosen_land_type.rs:107](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_land_type.rs:107) | decisions |
| `chooseCardName` | 5 | [src/oracle/canonical/effects/chosen_card_name.rs:133](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:133) | decisions |
| `chooseCards` | 555 | [src/oracle/canonical/effects/card_type_partition.rs:45](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/card_type_partition.rs:45) | effects, then, onPerformed |
| `chooseColor` | 249 | [src/oracle/canonical/abilities/restricted_mana_native.rs:77](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/restricted_mana_native.rs:77) | mana |
| `chooseCreatureType` | 51 | [src/oracle/canonical/effects/chosen_creature_type.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_creature_type.rs:60) | decisions |
| `chooseModes` | 53 | [src/oracle/canonical/abilities/additional_cost_native.rs:84](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/additional_cost_native.rs:84) | decisions |
| `chooseNumber` | 40 | [src/oracle/canonical/effects/delayed_draw.rs:91](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_draw.rs:91) | decisions, effects |
| `chooseObjects` | 298 | [src/oracle/canonical/primitive_helpers.rs:57](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/primitive_helpers.rs:57) | decisions |
| `chooseOrder` | 259 | [src/oracle/canonical/effects/draw_move.rs:111](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/draw_move.rs:111) | effects, then |
| `choosePermanents` | 3 | [src/oracle/canonical/effects/amass.rs:57](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/amass.rs:57) | effects, then |
| `chooseTargets` | 2217 | [src/oracle/canonical/automata/counter_named_token.rs:295](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/automata/counter_named_token.rs:295) | decisions |
| `chooseWhetherToPay` | 26 | [src/oracle/canonical/effects/chosen_land_type.rs:113](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_land_type.rs:113) | decisions |

## Paiement/coûts

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `discardCard` | 33 | [src/oracle/canonical/abilities/activated_native.rs:129](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:129) | additionalCosts, costs, then |
| `exileCard` | 17 | [src/oracle/canonical/abilities/alternative_cost_native.rs:148](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/alternative_cost_native.rs:148) | costs |
| `payLife` | 90 | [src/oracle/canonical/abilities/activated_native.rs:113](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:113) | costs, cost, then |
| `payLoyalty` | 4 | [src/oracle/canonical/effects/loyalty_ability.rs:23](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/loyalty_ability.rs:23) | costs |
| `payMana` | 2037 | [src/oracle/canonical/abilities/activated_native.rs:105](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:105) | costs, cost |
| `returnPermanentToOwnersHand` | 2 | [src/oracle/canonical/abilities/alternative_return_cost_native.rs:47](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/alternative_return_cost_native.rs:47) | costs |
| `tap` | 2450 | [src/oracle/canonical/abilities/activated_native.rs:109](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:109) | costs |

## Signaux sémantiques

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `emitAbilitySignal` | 251 | [src/oracle/canonical/effects/ability_signal.rs:17](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_signal.rs:17) | effects, then |
| `recordLibrarySearch` | 268 | [src/oracle/canonical/effects/library_search_move.rs:101](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/library_search_move.rs:101) | effects, then |

## Durée et timing

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `duringControllerTurn` | 1 | [src/oracle/canonical/effects/early_turn_entry.rs:41](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/early_turn_entry.rs:41) | operands |
| `oncePerTurn` | 4 | [src/oracle/canonical/effects/loyalty_ability.rs:112](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/loyalty_ability.rs:112) | activationLimit |
| `permanent` | 10 | [src/oracle/canonical/abilities/saga_native.rs:156](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:156) | operands, duration |
| `sorceryTiming` | 6 | [src/oracle/canonical/automata/counter_named_token.rs:375](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/automata/counter_named_token.rs:375) | activationCondition |
| `untilEndOfCurrentTurn` | 630 | [src/oracle/canonical/effects/ability_loss_destroy.rs:82](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_loss_destroy.rs:82) | duration |
| `untilNextTurn` | 3 | [src/oracle/canonical/effects/effect_sequence.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/effect_sequence.rs:60) | duration |

## Expressions et prédicats

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `add` | 18 | [src/oracle/canonical/effects/numeric.rs:73](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/numeric.rs:73) | toughness, quantity, amount |
| `and` | 652 | [src/oracle/canonical/ir.rs:55](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/ir.rs:55) | where, operands, operand, condition |
| `bindingNotEmpty` | 1 | [src/oracle/canonical/effects/delayed_blink.rs:133](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_blink.rs:133) | condition |
| `cardTypeContains` | 2763 | [src/oracle/canonical/automata/counter_named_token.rs:329](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/automata/counter_named_token.rs:329) | where, operands, operand, attacker |
| `colorContains` | 163 | [src/oracle/canonical/abilities/alternative_cost_native.rs:154](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/alternative_cost_native.rs:154) | operand, operands, where |
| `compare` | 216 | [src/oracle/canonical/effects/explore_program.rs:49](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/explore_program.rs:49) | operands, condition, where, operand |
| `conditionalValue` | 1 | [src/oracle/canonical/effects/conditional_top_card_partition.rs:69](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/conditional_top_card_partition.rs:69) | count |
| `controlsPermanent` | 36 | [src/oracle/canonical/abilities/conditional_mana_native.rs:56](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/conditional_mana_native.rs:56) | operand, activationCondition, condition |
| `costWasPaid` | 26 | [src/oracle/canonical/effects/condition.rs:24](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/condition.rs:24) | operand |
| `countCards` | 2 | [src/oracle/canonical/effects/condition.rs:53](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/condition.rs:53) | left |
| `countCounters` | 1 | [src/oracle/canonical/effects/landfall_reflexive_counter.rs:95](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/landfall_reflexive_counter.rs:95) | left |
| `countDecisionCardsMatching` | 1 | [src/oracle/canonical/effects/delve.rs:67](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delve.rs:67) | count |
| `countDistinctCardTypes` | 9 | [src/oracle/canonical/effects/numeric.rs:66](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/numeric.rs:66) | power, left |
| `countEventsThisTurn` | 1 | [src/oracle/canonical/effects/condition.rs:32](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/condition.rs:32) | left |
| `countObjects` | 256 | [src/oracle/canonical/effects/conditional_top_card_partition.rs:35](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/conditional_top_card_partition.rs:35) | maximum, operands |
| `countPermanents` | 68 | [src/oracle/canonical/abilities/saga_native.rs:52](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:52) | operands, left, power, toughness, amount, manaCostReduction |
| `countPlayerCounters` | 1 | [src/oracle/canonical/effects/player_counter_sweep.rs:64](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/player_counter_sweep.rs:64) | maximum |
| `countTurnsTaken` | 1 | [src/oracle/canonical/effects/early_turn_entry.rs:47](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/early_turn_entry.rs:47) | left |
| `divide` | 2 | [src/oracle/canonical/effects/numeric.rs:101](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/numeric.rs:101) | count |
| `eventQuantity` | 13 | [src/oracle/canonical/effects/numeric.rs:64](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/numeric.rs:64) | right, left |
| `hasKeyword` | 108 | [src/oracle/canonical/effects/object_predicate.rs:47](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/object_predicate.rs:47) | operand, operands |
| `haste` | 736 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `inZone` | 494 | [src/oracle/canonical/abilities/spell_counter_native.rs:36](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/spell_counter_native.rs:36) | activeWhile |
| `integer` | 4261 | [src/oracle/canonical/abilities/triggered_front_face.rs:198](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_front_face.rs:198) | quantity, power, toughness, value, count, right, amount, operands, finalChapter, minimum, left, maximum, basePower, baseToughness, drawOrdinal, chapters, spellCastOrdinal |
| `isActivatedAbility` | 1 | [src/oracle/canonical/effects/counter.rs:102](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter.rs:102) | operands |
| `isColorless` | 2 | [src/oracle/canonical/effects/counter.rs:108](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter.rs:108) | operands |
| `isCommander` | 7 | [src/oracle/canonical/effects/object_predicate.rs:22](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/object_predicate.rs:22) | where |
| `isLegendary` | 21 | [src/oracle/canonical/effects/object_predicate.rs:40](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/object_predicate.rs:40) | where, operands, operand |
| `isManaAbility` | 4 | [src/oracle/canonical/effects/chosen_card_name.rs:197](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:197) | operand |
| `isOpponentOf` | 44 | [src/oracle/canonical/effects/hand_disruption.rs:63](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/hand_disruption.rs:63) | where |
| `isPermanentCard` | 1 | [src/oracle/canonical/effects/owner_library_reveal.rs:54](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/owner_library_reveal.rs:54) | where |
| `isSpell` | 3 | [src/oracle/canonical/effects/counter.rs:107](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter.rs:107) | operands |
| `isToken` | 52 | [src/oracle/canonical/effects/graveyard_destination.rs:98](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/graveyard_destination.rs:98) | operand, where, operands |
| `isTriggeredAbility` | 2 | [src/oracle/canonical/effects/counter.rs:105](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter.rs:105) | operands |
| `islandwalk` | 1 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability |
| `manaCostEquals` | 1 | [src/oracle/canonical/effects/object_predicate.rs:43](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/object_predicate.rs:43) | operands |
| `manaValueOf` | 69 | [src/oracle/canonical/effects/graveyard_card_entry_life.rs:88](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/graveyard_card_entry_life.rs:88) | left, amount, value |
| `maximum` | 2 | [src/oracle/canonical/effects/exile.rs:195](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/exile.rs:195) | value |
| `minimum` | 10 | [src/oracle/canonical/effects/conditional_top_card_partition.rs:32](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/conditional_top_card_partition.rs:32) | minimum, count, ifFalse, ifTrue |
| `multiply` | 5 | [src/oracle/canonical/abilities/saga_native.rs:42](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:42) | quantity |
| `nameEquals` | 7 | [src/oracle/canonical/effects/chosen_card_name.rs:78](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:78) | sourceWhere, where |
| `not` | 415 | [src/oracle/canonical/effects/conditional_entry.rs:42](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/conditional_entry.rs:42) | condition, operands, where |
| `notYourTurn` | 5 | [src/oracle/canonical/effects/condition.rs:22](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/condition.rs:22) | condition |
| `objectMatchesFilter` | 6 | [src/oracle/canonical/effects/conditional_zone_change.rs:46](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/conditional_zone_change.rs:46) | condition |
| `oneOf` | 58 | [src/oracle/canonical/effects/entry_or_repeat_trigger.rs:99](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/entry_or_repeat_trigger.rs:99) | event |
| `or` | 421 | [src/oracle/canonical/ir.rs:51](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/ir.rs:51) | where, operands, operand |
| `playersEqual` | 1 | [src/oracle/canonical/effects/delayed_blink.rs:168](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_blink.rs:168) | condition |
| `powerOf` | 88 | [src/oracle/canonical/effects/exile.rs:197](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/exile.rs:197) | left, genericAmount, operands |
| `selectionContains` | 54 | [src/oracle/canonical/ir.rs:64](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/ir.rs:64) | condition |
| `sourceCastXValue` | 1 | [src/oracle/canonical/effects/numeric.rs:58](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/numeric.rs:58) | count, left |
| `subtypeContains` | 337 | [src/oracle/canonical/effects/amass.rs:27](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/amass.rs:27) | operands, where, operand |
| `toughnessOf` | 15 | [src/oracle/canonical/effects/numeric.rs:142](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/numeric.rs:142) | left, amount |
| `typeLineContains` | 163 | [src/oracle/canonical/effects/object_predicate.rs:37](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/object_predicate.rs:37) | operands, operand |
| `wasCastWithAlternativeCost` | 10 | [src/oracle/canonical/card_composition.rs:86](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/card_composition.rs:86) | operand, condition |
| `wasKicked` | 4 | [src/oracle/canonical/effects/condition.rs:17](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/condition.rs:17) | condition |

## Description de mana/restriction de dépense

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `castSpell` | 1 | [src/oracle/canonical/abilities/restricted_mana_native.rs:80](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/restricted_mana_native.rs:80) | spendRestriction |
| `chooseCommanderColor` | 4 | [src/oracle/canonical/effects/mana_lowering.rs:23](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/mana_lowering.rs:23) | mana |
| `chooseOne` | 521 | [src/oracle/canonical/effects/mana_lowering.rs:10](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/mana_lowering.rs:10) | mana |
| `manaColorsLandsOpponentsControlCouldProduce` | 4 | [src/oracle/canonical/effects/mana_lowering.rs:32](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/mana_lowering.rs:32) | mana |
| `manaTypesLandsYouControlCouldProduce` | 4 | [src/oracle/canonical/effects/mana_lowering.rs:27](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/mana_lowering.rs:27) | mana |

## Références, collections et zones

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `abilityController` | 2 | [src/oracle/canonical/effects/earthbend.rs:54](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/earthbend.rs:54) | controller, player |
| `activatedAbilities` | 6 | [src/oracle/canonical/effects/chosen_card_name.rs:210](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:210) | abilities |
| `anyGraveyard` | 79 | [src/oracle/canonical/effects/graveyard_card_entry_life.rs:76](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/graveyard_card_entry_life.rs:76) | zone |
| `anyTarget` | 234 | [src/oracle/canonical/effects/selection.rs:26](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/selection.rs:26) | candidates |
| `anyZone` | 7 | [src/oracle/canonical/effects/graveyard_destination.rs:111](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/graveyard_destination.rs:111) | from |
| `attachedPermanent` | 48 | [src/oracle/canonical/effects/attached_keyword_grant.rs:27](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/attached_keyword_grant.rs:27) | objects |
| `battlefield` | 548 | [src/oracle/canonical/effects/delayed_blink.rs:159](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_blink.rs:159) | to, zone |
| `boundObjects` | 270 | [src/oracle/canonical/abilities/triggered_front_face.rs:204](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_front_face.rs:204) | from, objects, left, cards, permanent |
| `boundValue` | 58 | [src/oracle/canonical/effects/ability_loss_destroy.rs:52](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_loss_destroy.rs:52) | controller, player, amount, object, left |
| `candidate` | 160 | [src/oracle/canonical/effects/numeric.rs:135](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/numeric.rs:135) | object |
| `cards` | 499 | [src/oracle/canonical/abilities/additional_cost_native.rs:101](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/additional_cost_native.rs:101) | candidates |
| `cardsExiledWithSource` | 1 | [src/oracle/canonical/effects/linked_exile_return.rs:59](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/linked_exile_return.rs:59) | objects |
| `cardsInZone` | 1 | [src/oracle/canonical/effects/exile.rs:104](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/exile.rs:104) | sets |
| `chosenObject` | 298 | [src/oracle/canonical/abilities/activated_native.rs:126](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:126) | permanent, card |
| `chosenTarget` | 2065 | [src/oracle/canonical/automata/counter_named_token.rs:309](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/automata/counter_named_token.rs:309) | object, permanent, recipient, card, controller, player, spell, nameFromCard, trackedObject |
| `chosenTargets` | 168 | [src/oracle/canonical/effects/activated_zone_change.rs:120](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/activated_zone_change.rs:120) | cards, permanent |
| `controllerOf` | 5511 | [src/oracle/canonical/abilities/restricted_mana_native.rs:73](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/restricted_mana_native.rs:73) | controller, player, recipient, value, players, exceptPlayer, graveyardOwners, duringTurnOf, chooser, left, right, protectedPlayer |
| `decisionOrder` | 259 | [src/oracle/canonical/effects/draw_move.rs:121](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/draw_move.rs:121) | order |
| `decisionResult` | 680 | [src/oracle/canonical/effects/delayed_draw.rs:102](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_draw.rs:102) | cards, amount, sets, objects, right, excludeCards, selection, count, from, left, maximum, quantity, minimum, permanent |
| `declaredAttacker` | 79 | [src/oracle/canonical/abilities/triggered_attack_mill.rs:80](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_attack_mill.rs:80) | event |
| `eachPermanent` | 161 | [src/oracle/canonical/effects/ability_loss_destroy.rs:71](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ability_loss_destroy.rs:71) | permanent, object, recipient, value |
| `eachPlayer` | 62 | [src/oracle/canonical/effects/cast_quota.rs:63](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/cast_quota.rs:63) | player, graveyardOwners, players, recipient |
| `eventObject` | 5 | [src/oracle/canonical/effects/trigger_suppression.rs:32](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/trigger_suppression.rs:32) | object |
| `exactly` | 298 | [src/oracle/canonical/abilities/activated_native.rs:44](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:44) | quantity |
| `exile` | 12 | [src/oracle/canonical/effects/delayed_blink.rs:155](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_blink.rs:155) | to, from |
| `filterObjects` | 2 | [src/oracle/canonical/effects/explore_program.rs:72](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/explore_program.rs:72) | cards |
| `graveyard` | 168 | [src/oracle/canonical/effects/explore_program.rs:93](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/explore_program.rs:93) | zone, from, to |
| `hand` | 293 | [src/oracle/canonical/abilities/additional_cost_native.rs:102](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/additional_cost_native.rs:102) | to, zone |
| `library` | 534 | [src/oracle/canonical/effects/card_type_partition.rs:105](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/card_type_partition.rs:105) | zone, to |
| `opponentsOf` | 306 | [src/oracle/canonical/effects/cast_prohibition.rs:36](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/cast_prohibition.rs:36) | controller, recipient, player, players, graveyardOwners |
| `ownerOf` | 7 | [src/oracle/canonical/effects/delayed_blink.rs:140](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_blink.rs:140) | value, player |
| `permanents` | 1927 | [src/oracle/canonical/abilities/activated_native.rs:40](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:40) | candidates, attackers, sets, objects |
| `player` | 2 | [src/oracle/canonical/effects/protected_attack_trigger.rs:15](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/protected_attack_trigger.rs:15) | operands |
| `players` | 158 | [src/oracle/canonical/effects/effect_sequence.rs:575](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/effect_sequence.rs:575) | candidates, sets |
| `playersExcept` | 12 | [src/oracle/canonical/effects/selection.rs:9](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/selection.rs:9) | controller |
| `sameZoneOwner` | 22 | [src/oracle/canonical/effects/activated_zone_change.rs:101](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/activated_zone_change.rs:101) | selectionConstraint |
| `self` | 17771 | [src/oracle/canonical/automata/counter_named_token.rs:286](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/automata/counter_named_token.rs:286) | source, object, permanent, blockers, attachment, objects, cards, card, spell |
| `setDifference` | 260 | [src/oracle/canonical/effects/card_type_partition.rs:33](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/card_type_partition.rs:33) | objects, cards, from, left |
| `singleBoundObject` | 1 | [src/oracle/canonical/effects/delayed_blink.rs:93](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delayed_blink.rs:93) | object |
| `spells` | 149 | [src/oracle/canonical/abilities/spell_counter_native.rs:57](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/spell_counter_native.rs:57) | candidates, sets, objects |
| `stack` | 86 | [src/oracle/canonical/abilities/spell_counter_native.rs:38](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/spell_counter_native.rs:38) | zone |
| `stackItems` | 3 | [src/oracle/canonical/effects/counter.rs:157](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/counter.rs:157) | candidates, targetingStackObject |
| `stackOrCast` | 2 | [src/oracle/canonical/effects/casting_reduction.rs:75](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/casting_reduction.rs:75) | zone |
| `storedDecision` | 7 | [src/oracle/canonical/effects/chosen_card_name.rs:80](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:80) | value |
| `triggeringDestination` | 2 | [src/oracle/canonical/effects/earthbend.rs:59](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/earthbend.rs:59) | from |
| `triggeringPermanent` | 2 | [src/oracle/canonical/effects/protected_attack_trigger.rs:67](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/protected_attack_trigger.rs:67) | object |
| `triggeringPlayer` | 4 | [src/oracle/canonical/effects/event_bound_payment.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/event_bound_payment.rs:60) | player |
| `union` | 56 | [src/oracle/canonical/effects/exile.rs:100](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/exile.rs:100) | objects, candidates, defender, object |

## Motifs d’événement

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `becameTarget` | 1 | [src/oracle/canonical/effects/targeting_trigger.rs:61](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/targeting_trigger.rs:61) | event |
| `cardDrawn` | 4 | [src/oracle/canonical/abilities/triggered_front_face.rs:196](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_front_face.rs:196) | event, events |
| `cardsLeftGraveyard` | 2 | [src/oracle/canonical/effects/delve.rs:123](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delve.rs:123) | event |
| `combatDamageToPlayer` | 81 | [src/oracle/canonical/effects/optional_bound_mill.rs:71](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/optional_bound_mill.rs:71) | event |
| `counterPlacement` | 10 | [src/oracle/canonical/effects/quantity_replacement.rs:58](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/quantity_replacement.rs:58) | event |
| `enterBattlefield` | 967 | [src/oracle/canonical/abilities/triggered_entry_earthbend.rs:119](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_entry_earthbend.rs:119) | event, events, causedBy |
| `landPlayed` | 1 | [src/oracle/canonical/effects/land_play_trigger.rs:68](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/land_play_trigger.rs:68) | event |
| `librarySearched` | 1 | [src/oracle/canonical/abilities/triggered_library.rs:79](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_library.rs:79) | event |
| `permanentAttacks` | 2 | [src/oracle/canonical/effects/protected_attack_trigger.rs:59](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/protected_attack_trigger.rs:59) | event |
| `permanentDied` | 225 | [src/oracle/canonical/effects/permanent_died_effect_sequence.rs:44](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/permanent_died_effect_sequence.rs:44) | event, events |
| `permanentEntered` | 217 | [src/oracle/canonical/effects/landfall_reflexive_counter.rs:174](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/landfall_reflexive_counter.rs:174) | event, events |
| `permanentSacrificed` | 12 | [src/oracle/canonical/effects/sacrifice_trigger_program.rs:94](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/sacrifice_trigger_program.rs:94) | event |
| `reflexiveTriggerCreated` | 1 | [src/oracle/canonical/effects/landfall_reflexive_counter.rs:122](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/landfall_reflexive_counter.rs:122) | event |
| `sagaChapterReached` | 8 | [src/oracle/canonical/abilities/saga_native.rs:186](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:186) | event |
| `spellCast` | 167 | [src/oracle/canonical/effects/ordinal_spell_tax_draw.rs:47](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/ordinal_spell_tax_draw.rs:47) | event |
| `stepBegan` | 212 | [src/oracle/canonical/effects/phase_trigger_sequence.rs:31](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/phase_trigger_sequence.rs:31) | event |
| `tokenCreation` | 3 | [src/oracle/canonical/effects/quantity_replacement.rs:58](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/quantity_replacement.rs:58) | event |
| `wouldEnterBattlefield` | 742 | [src/oracle/canonical/effects/chosen_card_name.rs:128](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:128) | event |

## Règles continues/remplacements

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `additionalCastingCost` | 25 | [src/oracle/canonical/effects/chosen_card_name.rs:168](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:168) | modifiers |
| `additionalLandPlay` | 22 | [src/oracle/canonical/effects/emblem.rs:25](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/emblem.rs:25) | modifiers |
| `attackTax` | 4 | [src/oracle/canonical/effects/attack_tax.rs:55](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/attack_tax.rs:55) | modifiers |
| `blockRestriction` | 35 | [src/oracle/canonical/effects/block_restriction.rs:68](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/block_restriction.rs:68) | modifiers |
| `cantBeCountered` | 96 | [src/oracle/canonical/abilities/spell_counter_native.rs:41](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/spell_counter_native.rs:41) | modifiers |
| `castLimitPerTurn` | 8 | [src/oracle/canonical/effects/cast_quota.rs:86](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/cast_quota.rs:86) | modifiers |
| `noMaximumHandSize` | 29 | [src/oracle/canonical/effects/emblem.rs:30](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/emblem.rs:30) | modifiers |
| `prohibitActivation` | 6 | [src/oracle/canonical/effects/chosen_card_name.rs:208](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:208) | modifiers |
| `prohibitCast` | 6 | [src/oracle/canonical/effects/cast_prohibition.rs:57](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/cast_prohibition.rs:57) | modifiers |
| `prohibitCastFromZones` | 4 | [src/oracle/canonical/effects/zone_prohibition.rs:90](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/zone_prohibition.rs:90) | modifiers |
| `prohibitEnterBattlefieldFromZones` | 2 | [src/oracle/canonical/effects/zone_prohibition.rs:110](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/zone_prohibition.rs:110) | modifiers |
| `prohibitUncastBattlefieldEntry` | 1 | [src/oracle/canonical/effects/uncast_entry_replacement.rs:103](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/uncast_entry_replacement.rs:103) | modifiers |
| `reduceCastingCost` | 88 | [src/oracle/canonical/effects/static_spell_reduction.rs:36](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/static_spell_reduction.rs:36) | modifiers |
| `reduceOwnGenericCastingCost` | 2 | [src/oracle/canonical/effects/casting_reduction.rs:78](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/casting_reduction.rs:78) | modifiers |
| `replaceEventQuantity` | 13 | [src/oracle/canonical/effects/quantity_replacement.rs:90](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/quantity_replacement.rs:90) | modifiers |
| `replaceGraveyardDestination` | 7 | [src/oracle/canonical/effects/graveyard_destination.rs:123](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/graveyard_destination.rs:123) | modifiers |
| `retainChosenBasicLandType` | 1 | [src/oracle/canonical/effects/chosen_land_type.rs:172](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_land_type.rs:172) | modifiers |
| `setBasePowerToughness` | 9 | [src/oracle/canonical/effects/graveyard_characteristic.rs:62](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/graveyard_characteristic.rs:62) | modifiers |
| `suppressTriggerCreation` | 5 | [src/oracle/canonical/effects/trigger_suppression.rs:49](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/trigger_suppression.rs:49) | modifiers |

## Enveloppes, mots-clés et métadonnées

| Kind | Capacités natives contenant ce nœud | Émetteur de référence | Rôles JSON observés |
|---|---:|---|---|
| `activatedAbility` | 1195 | [src/oracle/canonical/abilities/activated_library_search_native.rs:85](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_library_search_native.rs:85) | rule, ability |
| `alternativeCost` | 51 | [src/oracle/canonical/abilities/alternative_cost_native.rs:51](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/alternative_cost_native.rs:51) | ability |
| `cascade` | 34 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability |
| `castingDeclaration` | 2989 | [src/oracle/canonical/abilities/activated_native.rs:242](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/activated_native.rs:242) | declaration |
| `deathtouch` | 382 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `defender` | 327 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `delve` | 28 | [src/oracle/canonical/effects/delve.rs:21](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/delve.rs:21) | ability |
| `doubleStrike` | 133 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `equip` | 612 | [src/oracle/canonical/abilities/equip_native.rs:34](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/equip_native.rs:34) | ability |
| `exalted` | 35 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability |
| `firstStrike` | 392 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `flash` | 646 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `flying` | 3394 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `forestwalk` | 1 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability |
| `hexproof` | 89 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `horsemanship` | 1 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability |
| `indestructible` | 114 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `infect` | 1 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | abilities |
| `keywordAbility` | 8259 | [src/oracle/canonical/abilities/alternative_cost_native.rs:48](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/alternative_cost_native.rs:48) | rule |
| `keywordAbilityGroup` | 1119 | [src/oracle/canonical/effects/simple_keyword.rs:78](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:78) | rule |
| `kicker` | 198 | [src/oracle/canonical/abilities/keyword_native.rs:46](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/keyword_native.rs:46) | ability |
| `lifelink` | 414 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `manaAbility` | 1795 | [src/oracle/canonical/abilities/conditional_mana_native.rs:97](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/conditional_mana_native.rs:97) | rule, abilities, ability |
| `menace` | 445 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `namedToken` | 193 | [src/oracle/canonical/abilities/triggered_front_face.rs:104](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/triggered_front_face.rs:104) | token |
| `openingHandBattlefield` | 19 | [src/oracle/canonical/effects/opening_hand_battlefield.rs:58](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/opening_hand_battlefield.rs:58) | ability |
| `persist` | 24 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability |
| `plainswalk` | 1 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability |
| `prowess` | 105 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `random` | 8 | [src/oracle/canonical/effects/card_type_partition.rs:109](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/card_type_partition.rs:109) | selection, selectionProvenance, order |
| `reach` | 457 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `replacementEffect` | 742 | [src/oracle/canonical/effects/chosen_card_name.rs:125](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/chosen_card_name.rs:125) | rule |
| `replicate` | 16 | [src/oracle/canonical/abilities/replicate_native.rs:37](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/replicate_native.rs:37) | ability |
| `rulesMarker` | 219 | [src/oracle/canonical/abilities/phyrexian_mana_native.rs:35](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/phyrexian_mana_native.rs:35) | rule |
| `shroud` | 1 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability |
| `spellAbility` | 2021 | [src/oracle/canonical/abilities/additional_cost_native.rs:137](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/additional_cost_native.rs:137) | rule |
| `staticAbility` | 495 | [src/oracle/canonical/abilities/saga_native.rs:57](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:57) | rule, abilities |
| `trample` | 1066 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `transformFace` | 1 | [src/oracle/canonical/effects/source_move.rs:33](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/source_move.rs:33) | faceChange |
| `triggeredAbility` | 1931 | [src/oracle/canonical/abilities/saga_native.rs:183](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/abilities/saga_native.rs:183) | rule, ability |
| `vigilance` | 800 | [src/oracle/canonical/effects/simple_keyword.rs:60](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:60) | ability, abilities |
| `ward` | 162 | [src/oracle/canonical/effects/simple_keyword.rs:56](D:/User/Documents/Projet/deepdeck-engine/src/oracle/canonical/effects/simple_keyword.rs:56) | abilities, ability |

## Constructeurs présents dans le code mais absents de cet audit

`chosenCreatureType`, `controllerOfObject`, `objectOnBattlefield`, `replaceTargetWithEach`, `subtract`, `tapPermanent`. Leur absence n’établit ni code mort ni absence de support. En particulier replaceTargetWithEach est une représentation intermédiaire consommée par la composition Overload.
