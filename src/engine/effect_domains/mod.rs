//! Private routing of effect validation and execution by semantic domain.
//! This table is mechanical: it neither changes the IR nor certifies native coverage.
//! Existing root preflight/checkpoint and earlier domain hooks retain precedence.
use super::*;

mod battlefield_deployment;
mod card_movement;
mod casting_permissions;
mod characteristic_layers;
mod characteristics;
mod choices;
mod continuous_rules;
mod control;
mod cost_transactions;
mod counters;
mod damage_combat;
mod destruction_sacrifice;
mod exile;
mod graveyard_reanimation;
mod hand;
mod keyword_modification;
mod library;
mod library_observation;
mod linked_exile;
mod mana;
mod movement;
mod payments;
mod permanent_movement;

mod player_status;
mod reflexive_cost_transactions;
mod rules_permissions;

mod stack_casting;
mod stack_copying;
mod tokens_copies;
mod triggers;
mod type_modification;

pub(super) use characteristic_layers::{
    battlefield_object_operand_supported, characteristic_object_supported,
    marker_base_power_toughness, marker_card_types, marker_colors,
};

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    super::card_zone_transition::node_requires_checkpoint(value)
        || graveyard_reanimation::node_requires_checkpoint(value)
        || battlefield_deployment::node_requires_checkpoint(value)
        || permanent_movement::node_requires_checkpoint(value)
        || destruction_sacrifice::node_requires_checkpoint(value)
        || control::node_requires_checkpoint(value)
        || card_movement::node_requires_checkpoint(value)
        || matches!(
            value_kind(value),
            Some("revealCards" | "optionalPayCostCreateReflexiveTrigger")
        )
        || (matches!(value_kind(value), Some("drawCards" | "mill"))
            && integer_value(&value["count"]).is_none())
}

#[derive(Clone, Copy)]
enum Domain {
    Control,
    Payments,

    CostTransactions,
    ReflexiveCostTransactions,

    Choices,
    Library,
    LibraryObservation,
    Hand,
    Movement,
    GraveyardReanimation,
    CardMovement,
    BattlefieldDeployment,
    PermanentMovement,
    Exile,
    LinkedExile,
    DestructionSacrifice,
    DamageCombat,
    Counters,
    KeywordModification,
    CharacteristicLayers,
    Characteristics,
    TypeModification,
    TokensCopies,
    RulesPermissions,
    ContinuousRules,
    PlayerStatus,
    CastingPermissions,
    Triggers,
    Mana,
    StackCasting,
    StackCopying,
}

fn domain(effect: &Value) -> Option<Domain> {
    match value_kind(effect) {
        Some(
            "forEachObject" | "chooseOneEffect" | "optionalEffects" | "optionalAction"
            | "performAction" | "bind" | "conditional" | "conditionalEffect" | "ifTargetWasChosen"
            | "chooseUnusedMode",
        ) => Some(Domain::Control),
        Some("optionalPayLife") => Some(Domain::Payments),

        Some("optionalPayCostPerformEffects" | "tryPayCost") => Some(Domain::CostTransactions),
        Some("optionalPayCostCreateReflexiveTrigger") => Some(Domain::ReflexiveCostTransactions),

        Some(
            "choosePlayers"
            | "chooseNumber"
            | "chooseCardName"
            | "chooseCreatureType"
            | "chooseColor"
            | "chooseColors"
            | "chooseBasicLandType"
            | "choosePermanents"
            | "chooseCards"
            | "chooseOrder",
        ) => Some(Domain::Choices),
        Some(
            "drawCards"
            | "revealCards"
            | "mill"
            | "recordLibrarySearch"
            | "shuffleZone"
            | "exileFromTopUntil",
        ) => Some(Domain::Library),
        Some("lookAtTopCards" | "revealFromTopUntilCount") => Some(Domain::LibraryObservation),
        Some(
            "discardCards"
            | "exileCardsFromHand"
            | "discardHand"
            | "lookAtRandomCardInHand"
            | "revealHand"
            | "lookAtHand",
        ) => Some(Domain::Hand),
        Some(
            "moveTriggeringCardFromGraveyard"
            | "moveAbilitySourceToHand"
            | "returnLinkedExiledCardToHand"
            | "returnAbilitySourceFromGraveyard",
        ) => Some(Domain::Movement),
        Some("returnCardsFromGraveyard") => Some(Domain::GraveyardReanimation),
        Some("moveCards" | "moveCard" | "move" | "moveTargetCard") => Some(Domain::CardMovement),
        Some("moveAbilitySourceToBattlefield" | "putCardsFromHandOntoBattlefield") => {
            Some(Domain::BattlefieldDeployment)
        }
        Some(
            "blinkPermanent"
            | "blinkPermanents"
            | "returnToOwnersHand"
            | "putPermanentOnOwnerLibrary"
            | "putAbilitySourceOnOwnersLibrary"
            | "movePermanentsToOwnersLibraries"
            | "returnPermanentsToOwnersHands"
            | "putPermanentOnBottomOwnersLibrary",
        ) => Some(Domain::PermanentMovement),
        Some(
            "exileCards"
            | "exilePermanent"
            | "exilePlayerGraveyard"
            | "exileChosenPlayersGraveyards"
            | "exileTogether"
            | "exileAbilitySourceFromGraveyard"
            | "exileTargetCards",
        ) => Some(Domain::Exile),
        Some("exileLinkedTokens") => Some(Domain::LinkedExile),
        Some(
            "destroyPermanent"
            | "sacrificePermanent"
            | "destroyChosenPermanents"
            | "sacrificePermanents"
            | "sacrificePermanentsEachPlayer"
            | "sacrificeAbilitySource",
        ) => Some(Domain::DestructionSacrifice),
        Some(
            "dealDamage"
            | "dealDividedDamage"
            | "fightPermanents"
            | "chosenPermanentsDealPowerDamage"
            | "tapPermanent"
            | "tapPermanents"
            | "untapPermanent"
            | "untapPermanents"
            | "untapPermanentsMatching"
            | "untapAttackingPermanents"
            | "addCombatPhase",
        ) => Some(Domain::DamageCombat),
        Some(
            "putCounters"
            | "putCountersOnCreaturesOfTargetPlayer"
            | "putSameCountersAs"
            | "doubleCounters"
            | "doubleAllCountersOnPermanents"
            | "removeCounters"
            | "removeUpToCounters"
            | "removeAnyNumberOfCounters"
            | "blightPlayer"
            | "blightEachOpponent"
            | "removeAllCounters"
            | "addPlayerCounters"
            | "removePlayerCounters"
            | "proliferate",
        ) => Some(Domain::Counters),
        Some(
            "removeKeywordsFromPermanents"
            | "installLoseAllAbilities"
            | "grantAbility"
            | "replaceAbility"
            | "grantProtection"
            | "grantPlayerProtection"
            | "loseAllAbilitiesPermanently"
            | "grantSourceOtherAbilities"
            | "grantKeyword"
            | "grantKeywordToTargets"
            | "grantKeywordToAttackingPermanents"
            | "grantKeywordToTriggeringAttackers",
        ) => Some(Domain::KeywordModification),
        Some(
            "modifyPowerToughness"
            | "setPower"
            | "grantFirebending"
            | "setPermanentFlag"
            | "suspectPermanent",
        ) => Some(Domain::Characteristics),
        Some("setCardTypes" | "setColors" | "setBasePowerToughness") => {
            Some(Domain::CharacteristicLayers)
        }
        Some(
            "transformPermanent"
            | "modifySubtypeSet"
            | "becomeCreature"
            | "addSubtypeToPermanent"
            | "setPermanentCardTypes"
            | "setAllColorsUntilEndOfTurn"
            | "setColorlessUntilEndOfTurn"
            | "addPermanentCharacteristics"
            | "removeAllSubtypesUntilEndOfTurn"
            | "addCardType",
        ) => Some(Domain::TypeModification),
        Some(
            "createCardCopy"
            | "ceaseToExist"
            | "conjureNamedCard"
            | "becomeCopyOfPermanent"
            | "createTokens",
        ) => Some(Domain::TokensCopies),
        Some(
            "setPrepared"
            | "retainUnspentMana"
            | "installLifeForManaPermission"
            | "ignoreLegendRuleUntilEndOfTurn"
            | "gainControlPermanent"
            | "exchangeControl"
            | "phaseOutPermanent"
            | "preventAttacksUntilNextTurn"
            | "preventPlayerDamageUntilNextTurn"
            | "suspendPermanent"
            | "gainControlWhileSourceTapped"
            | "setClassLevel"
            | "createEmblemPermanentModifier"
            | "createEmblem"
            | "replaceGraveyardWithExileUntilEndOfTurn"
            | "goadAllCreaturesUntilNextTurn"
            | "goadPermanents"
            | "prohibitPermanentActivations",
        ) => Some(Domain::RulesPermissions),
        Some(
            "installUntapRestriction"
            | "installRegenerationShield"
            | "installRegenerationShields"
            | "installDeathExileReplacement"
            | "installRuleModifier"
            | "installCreatureDeathLifeGainUntilEndOfTurn"
            | "installDamageMultiplier"
            | "preventCombatDamageFromPermanent"
            | "preventDamage"
            | "installCounterLinkedCharacteristics"
            | "increaseOpponentSpellCostUntilNextTurn"
            | "installAdditionalManaOnLandTap"
            | "installLinkedKeyword"
            | "installLinkedAbilityLoss"
            | "installLinkedDamagePrevention"
            | "installOneShotModifier",
        ) => Some(Domain::ContinuousRules),
        Some(
            "grantNoMaximumHandSize"
            | "loseGame"
            | "grantExtraTurn"
            | "ventureDungeon"
            | "restrictPlayerActions",
        ) => Some(Domain::PlayerStatus),
        Some(
            "installNextCreatureSpellModifier"
            | "installTemporaryCantBeCountered"
            | "grantCardPermission"
            | "prohibitAbilityClassDuringGrantedExtraTurn"
            | "grantAdditionalLandPlays"
            | "grantZoneCastingUntilEndOfTurn"
            | "grantHandPlayPermission"
            | "replaceCastSpellDestination"
            | "grantPermission",
        ) => Some(Domain::CastingPermissions),
        Some(
            "installAttackTrigger"
            | "installOpponentAttackTrigger"
            | "installDelayedTriggeredAbility"
            | "installCombatDamageTrigger"
            | "createReflexiveTrigger"
            | "scheduleSacrificeAtEndCombat"
            | "sacrificeAtNextEndStep"
            | "resolveOrdinalTriggeredAbility"
            | "installDelayedSpellCastTrigger"
            | "installDelayedStepTrigger",
        ) => Some(Domain::Triggers),
        Some("addMana" | "addFixedManaSymbols") => Some(Domain::Mana),
        Some(
            "castAnyNumber"
            | "castParadigmCopy"
            | "attachPermanent"
            | "castOneCard"
            | "counterSpell"
            | "counterStackObject"
            | "counterOpponentStackObjects"
            | "suspendStackItem"
            | "changeSingleTarget"
            | "changeTargets",
        ) => Some(Domain::StackCasting),
        Some("copyResolvingSpellIfCastFromZone" | "copyStackItem" | "copyResolvingStackObject") => {
            Some(Domain::StackCopying)
        }
        _ => None,
    }
}

pub(super) fn effect_domain_supported(effect: &Value) -> Option<bool> {
    match domain(effect)? {
        Domain::Control => control::supports_effect(effect),
        Domain::Payments => payments::supports_effect(effect),

        Domain::CostTransactions => cost_transactions::supports_effect(effect),
        Domain::ReflexiveCostTransactions => reflexive_cost_transactions::supports_effect(effect),

        Domain::Choices => choices::supports_effect(effect),
        Domain::Library => library::supports_effect(effect),
        Domain::LibraryObservation => library_observation::supports_effect(effect),
        Domain::Hand => hand::supports_effect(effect),
        Domain::Movement => movement::supports_effect(effect),
        Domain::GraveyardReanimation => graveyard_reanimation::supports_effect(effect),
        Domain::CardMovement => card_movement::supports_effect(effect),
        Domain::BattlefieldDeployment => battlefield_deployment::supports_effect(effect),
        Domain::PermanentMovement => permanent_movement::supports_effect(effect),
        Domain::Exile => exile::supports_effect(effect),
        Domain::LinkedExile => linked_exile::supports_effect(effect),
        Domain::DestructionSacrifice => destruction_sacrifice::supports_effect(effect),
        Domain::DamageCombat => damage_combat::supports_effect(effect),
        Domain::Counters => counters::supports_effect(effect),
        Domain::KeywordModification => keyword_modification::supports_effect(effect),
        Domain::CharacteristicLayers => characteristic_layers::supports_effect(effect),
        Domain::Characteristics => characteristics::supports_effect(effect),
        Domain::TypeModification => type_modification::supports_effect(effect),
        Domain::TokensCopies => tokens_copies::supports_effect(effect),
        Domain::RulesPermissions => rules_permissions::supports_effect(effect),
        Domain::ContinuousRules => continuous_rules::supports_effect(effect),
        Domain::PlayerStatus => player_status::supports_effect(effect),
        Domain::CastingPermissions => casting_permissions::supports_effect(effect),
        Domain::Triggers => triggers::supports_effect(effect),
        Domain::Mana => mana::supports_effect(effect),
        Domain::StackCasting => stack_casting::supports_effect(effect),
        Domain::StackCopying => stack_copying::supports_effect(effect),
    }
}

impl GameEngine {
    pub(super) fn execute_effect_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        let Some(domain) = domain(effect) else {
            return Ok(false);
        };
        match domain {
            Domain::Control => self.execute_control_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Payments => self.execute_payments_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),

            Domain::CostTransactions => self.execute_cost_transactions_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::ReflexiveCostTransactions => self.execute_reflexive_cost_transactions_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),

            Domain::Choices => self.execute_choices_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Library => self.execute_library_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::LibraryObservation => self.execute_library_observation_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Hand => self.execute_hand_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Movement => self.execute_movement_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::GraveyardReanimation => self.execute_graveyard_reanimation_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::CardMovement => self.execute_card_movement_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::BattlefieldDeployment => self.execute_battlefield_deployment_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::PermanentMovement => self.execute_permanent_movement_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Exile => self.execute_exile_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::LinkedExile => self.execute_linked_exile_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::DestructionSacrifice => self.execute_destruction_sacrifice_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::DamageCombat => self.execute_damage_combat_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Counters => self.execute_counters_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::KeywordModification => self.execute_keyword_modification_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::CharacteristicLayers => self.execute_characteristic_layers_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Characteristics => self.execute_characteristics_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::TypeModification => self.execute_type_modification_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::TokensCopies => self.execute_tokens_copies_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::RulesPermissions => self.execute_rules_permissions_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::ContinuousRules => self.execute_continuous_rules_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::PlayerStatus => self.execute_player_status_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::CastingPermissions => self.execute_casting_permissions_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Triggers => self.execute_triggers_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Mana => self.execute_mana_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::StackCasting => self.execute_stack_casting_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::StackCopying => self.execute_stack_copying_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "tests.rs"]
mod tests;

/// Movement owned by complete child frames must not be published again by a control parent.
pub(super) fn movement_is_owned_by_child_frames(effect: &Value) -> bool {
    control::movement_is_owned_by_child_frames(effect)
}
