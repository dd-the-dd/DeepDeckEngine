//! Private routing of effect validation and execution by semantic domain.
//! This table is mechanical: it neither changes the IR nor certifies native coverage.
//! Existing root preflight/checkpoint and earlier domain hooks retain precedence.
use super::*;

mod attachment;
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
mod face_status;
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

mod combat_removal;
pub(super) mod player_designations;
mod player_status;
mod reflexive_cost_transactions;
mod room_doors;
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
pub(super) use face_status::TURNED_FACE_DOWN_BY_EFFECT;

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
    PlayerDesignations,
    CombatRemoval,
    FaceStatus,
    Attachment,
    RoomDoors,
    CastingPermissions,
    Triggers,
    Mana,
    StackCasting,
    StackCopying,
}

fn domain(effect: &Value) -> Option<Domain> {
    match value_kind(effect) {
        Some(
            "forEachObject" | "repeatEffects" | "chooseOneEffect" | "optionalEffects"
            | "optionalAction" | "performAction" | "bind" | "conditional" | "conditionalEffect"
            | "ifTargetWasChosen" | "chooseUnusedMode",
        ) => Some(Domain::Control),
        Some("optionalPayLife" | "payCost") => Some(Domain::Payments),

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
            | "chooseOption"
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
            | "lookAtHand"
            | "lookAtFaceDownPermanents"
            | "grantCardVisibility",
        ) => Some(Domain::Hand),
        Some(
            "moveTriggeringCardFromGraveyard"
            | "moveAbilitySourceToHand"
            | "returnLinkedExiledCardToHand"
            | "returnAbilitySourceFromGraveyard"
            | "shuffleLibrariesHolding",
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
            | "putChosenCounters"
            | "putDividedCounters"
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
        Some("setCardTypes" | "setColors" | "setBasePowerToughness" | "switchPowerToughness") => {
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
            | "installGraveyardDestinationReplacement"
            | "installQuantityReplacement"
            | "installDamageReplacement"
            | "installLeaveBattlefieldExileReplacement"
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
            | "installOneShotModifier"
            | "installCombatRequirement"
            | "lockLifeTotal",
        ) => Some(Domain::ContinuousRules),
        Some(
            "grantNoMaximumHandSize"
            | "grantPlayerKeyword"
            | "loseGame"
            | "winGame"
            | "grantExtraTurn"
            | "ventureDungeon"
            | "restrictPlayerActions",
        ) => Some(Domain::PlayerStatus),
        Some("becomeMonarch" | "takeInitiative" | "temptWithRing" | "skipTurnPart") => {
            Some(Domain::PlayerDesignations)
        }
        Some("removeFromCombat") => Some(Domain::CombatRemoval),
        Some("turnPermanentFaceUp" | "turnPermanentFaceDown") => Some(Domain::FaceStatus),
        Some("unattachPermanent") => Some(Domain::Attachment),
        Some("changeRoomDoor") => Some(Domain::RoomDoors),
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
        Domain::PlayerDesignations => player_designations::supports_effect(effect),
        Domain::CombatRemoval => combat_removal::supports_effect(effect),
        Domain::FaceStatus => face_status::supports_effect(effect),
        Domain::Attachment => attachment::supports_effect(effect),
        Domain::RoomDoors => room_doors::supports_effect(effect),
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
            Domain::PlayerDesignations => self.execute_player_designations_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::CombatRemoval => self.execute_combat_removal_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::FaceStatus => self.execute_face_status_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::Attachment => self.execute_attachment_domain(
                effect,
                next_effect,
                stack_object,
                bindings,
                decisions,
                provider,
            ),
            Domain::RoomDoors => self.execute_room_doors_domain(
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

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "bound_object_lki_tests.rs"]
mod bound_object_lki_tests;

/// Movement owned by complete child frames must not be published again by a control parent.
pub(super) fn movement_is_owned_by_child_frames(effect: &Value) -> bool {
    control::movement_is_owned_by_child_frames(effect)
}

/// The decision key holding the last known information (CR 608.2h) of the
/// permanents one producing action bound under `binding` and removed from
/// the battlefield, keyed by object id (as `#identities` keys a capture).
pub(in crate::engine) fn bound_object_lki_key(binding: &str) -> String {
    format!("{binding}#lki")
}

/// Runtime actions whose `bind` records permanents they themselves remove
/// from the battlefield: their values must be read before the removal.
fn removes_bound_permanents(effect: &Value) -> bool {
    matches!(
        value_kind(effect),
        Some("sacrificePermanent" | "sacrificePermanents" | "destroyPermanent" | "exilePermanent")
    ) && effect["bind"].is_string()
}

/// A printed power or toughness of a card outside the battlefield (CR
/// 208.1); a card without one reads 0.
fn printed_stat(value: &Option<String>) -> i32 {
    value
        .as_deref()
        .and_then(|stat| stat.trim().parse::<i32>().ok())
        .unwrap_or(0)
}

impl GameEngine {
    /// The characteristics every battlefield permanent has before a
    /// producing action that removes the permanents it binds (sacrifice,
    /// destruction, exile). `None` for any other action: nothing to keep.
    pub(super) fn bound_object_lki_snapshot(
        &self,
        effect: &Value,
    ) -> Option<BTreeMap<String, Value>> {
        if !removes_bound_permanents(effect) {
            return None;
        }
        Some(
            self.state
                .players
                .iter()
                .flat_map(|player| player.battlefield.iter())
                .map(|permanent| {
                    (
                        permanent.instance_id.clone(),
                        json!({
                            "power": self.current_power(permanent),
                            "toughness": self.current_toughness(permanent),
                            "manaValue": mana_value(&permanent.definition),
                            "controller": permanent.controller,
                            "owner": permanent.owner,
                        }),
                    )
                })
                .collect(),
        )
    }

    /// After the producing action ran: the last known information of each
    /// bound object that left the battlefield, as it last existed there
    /// (CR 608.2h), recorded for the readers of the same resolution.
    pub(super) fn record_bound_object_lki(
        &self,
        effect: &Value,
        snapshot: Option<BTreeMap<String, Value>>,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
    ) {
        let (Some(snapshot), Some(binding)) = (snapshot, effect["bind"].as_str()) else {
            return;
        };
        let Some(RuntimeBinding::Objects(objects)) = bindings.get(binding) else {
            return;
        };
        let departed = objects
            .iter()
            .filter(|instance_id| self.permanent_position(instance_id).is_none())
            .filter_map(|instance_id| {
                snapshot
                    .get(instance_id)
                    .map(|values| (instance_id.clone(), values.clone()))
            })
            .collect::<serde_json::Map<String, Value>>();
        if !departed.is_empty() {
            decisions.insert(bound_object_lki_key(binding), Value::Object(departed));
        }
    }

    /// `powerOf|toughnessOf|manaValueOf` of the one object a producing
    /// action bound (CR 608.2c): its recorded last known information once
    /// it left the battlefield (CR 608.2h), else the printed values of the
    /// card it is outside the battlefield; 0 when the action bound nothing
    /// (no object was sacrificed, discarded or revealed). `None` when this
    /// reader does not own the expression (a live permanent, another
    /// object kind, an unbound or captured binding).
    pub(super) fn bound_object_numeric_read(
        &self,
        expression: &Value,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Option<Option<i32>> {
        let field = match value_kind(expression)? {
            "powerOf" => "power",
            "toughnessOf" => "toughness",
            "manaValueOf" => "manaValue",
            _ => return None,
        };
        let object = &expression["object"];
        if !matches!(
            value_kind(object),
            Some("singleBoundObject" | "boundObject")
        ) {
            return None;
        }
        let binding = object["binding"].as_str()?;
        let objects = match bindings.get(binding) {
            Some(RuntimeBinding::Objects(objects)) => objects.clone(),
            Some(_) => return None,
            // The card a resolution choice of this resolution selected (a
            // card revealed at random): the decision records its id. A
            // delayed trigger's captured binding keeps its own reader.
            None if !decisions
                .contains_key(&super::captured_binding::captured_identities_key(binding)) =>
            {
                decisions
                    .get(binding)?
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            }
            None => return None,
        };
        let Some(instance_id) = objects.first() else {
            return Some(Some(0));
        };
        if let Some(value) = decisions
            .get(&bound_object_lki_key(binding))
            .and_then(|records| records.get(instance_id.as_str()))
            .and_then(|record| record.get(field))
            .and_then(Value::as_i64)
        {
            return Some(i32::try_from(value).ok());
        }
        if self.permanent_position(instance_id).is_some() {
            return None;
        }
        let card = self.card_outside_battlefield(instance_id)?;
        Some(Some(match field {
            "manaValue" => mana_value(&card.definition),
            "power" => printed_stat(&card.definition.power),
            _ => printed_stat(&card.definition.toughness),
        }))
    }
}
