use super::super::*;

/// These are execution facts about the typed action, never a recursive search
/// through token definitions or granted abilities. Creating a Treasure does
/// not produce mana while this program resolves. New composite action nodes
/// must explicitly describe their execution semantics here.
pub(in crate::oracle::canonical) fn produces_mana_while_resolving(
    effect: &LocalEffectNode,
) -> bool {
    match effect {
        LocalEffectNode::OptionalSequence(body) => {
            body.effects.iter().any(produces_mana_while_resolving)
        }
        LocalEffectNode::CardAction(_) => false,
        LocalEffectNode::AddMana(_) => true,
        LocalEffectNode::Retarget(_)
        | LocalEffectNode::CounterSpell(_)
        | LocalEffectNode::TemporaryKeywordGrant(_)
        | LocalEffectNode::DealDamage(_)
        | LocalEffectNode::DrawCards(_)
        | LocalEffectNode::OptionalDrawCards(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::Amass(_)
        | LocalEffectNode::EmpowerJace(_)
        | LocalEffectNode::Investigate(_)
        | LocalEffectNode::ControllerKeywordAction(_)
        | LocalEffectNode::DiscardCards(_)
        | LocalEffectNode::Scry(_)
        | LocalEffectNode::Surveil(_)
        | LocalEffectNode::Earthbend(_)
        | LocalEffectNode::TransformPermanent(_)
        | LocalEffectNode::PutCounters(_)
        | LocalEffectNode::MoveSource(_)
        | LocalEffectNode::MoveCards(_)
        | LocalEffectNode::ZoneBatch(_)
        | LocalEffectNode::ZoneChange(_)
        | LocalEffectNode::TargetCardZoneChange(_)
        | LocalEffectNode::CreateExplicitTokens(_)
        | LocalEffectNode::CreateTokens(_)
        | LocalEffectNode::CreateNamedTokens(_)
        | LocalEffectNode::OptionalLibrarySearch(_)
        | LocalEffectNode::ControllerLibrarySearch(_)
        | LocalEffectNode::OptionalCardDeployment(_)
        | LocalEffectNode::GainLife(_)
        | LocalEffectNode::LoseLife(_)
        | LocalEffectNode::PlayerAction(_)
        | LocalEffectNode::TargetPlayerLife(_)
        | LocalEffectNode::LoseAbilities(_)
        | LocalEffectNode::DestroyReferencedSet(_)
        | LocalEffectNode::DestroyCollection(_)
        | LocalEffectNode::Sacrifice(_)
        | LocalEffectNode::ModifyStats(_)
        | LocalEffectNode::BecomeTypes(_)
        | LocalEffectNode::QuantifiedPlayerAction(_)
        | LocalEffectNode::TemporaryPlayerPermission(_)
        | LocalEffectNode::SelectedPermanents(_)
        | LocalEffectNode::TapPermanents(_)
        | LocalEffectNode::PermanentKeywordAction(_)
        | LocalEffectNode::ControlChange(_)
        | LocalEffectNode::CopyStackObject(_)
        | LocalEffectNode::PreventDamage(_)
        | LocalEffectNode::Attach(_)
        | LocalEffectNode::Choose(_)
        | LocalEffectNode::UntapTarget(_)
        | LocalEffectNode::Fight(_) => false,
    }
}

/// CR605.1a (2026-11-06): current execution must not move cards to or
/// from a library. Granted/token abilities and reads of library size do not
/// execute here. Each action and each zone enum is matched exhaustively so
/// future grammar additions must explicitly describe this execution fact.
pub(in crate::oracle::canonical) fn moves_cards_to_or_from_library(
    effect: &LocalEffectNode,
) -> bool {
    match effect {
        LocalEffectNode::PlayerAction(node) => match &node.action {
            SingularPlayerAction::Draw(_) | SingularPlayerAction::Mill(_) => true,
            SingularPlayerAction::Life(_)
            | SingularPlayerAction::Discard(_)
            | SingularPlayerAction::Sacrifice(_)
            | SingularPlayerAction::Venture(_)
            | SingularPlayerAction::Proliferate
            | SingularPlayerAction::Hand(_)
            | SingularPlayerAction::ReferencedCards { .. } => false,
        },
        LocalEffectNode::OptionalSequence(body) => {
            body.effects.iter().any(moves_cards_to_or_from_library)
        }
        LocalEffectNode::CardAction(action) => match action {
            CardActionNode::Look { .. }
            | CardActionNode::Reveal(_)
            | CardActionNode::RevealUntil { .. }
            | CardActionNode::Choose(_)
            | CardActionNode::Permit(_) => false,
            CardActionNode::Shuffle { .. } => true,
            CardActionNode::Move { destination, .. } => destination.zone != CardZoneName::Library,
        },
        LocalEffectNode::DrawCards(_)
        | LocalEffectNode::OptionalDrawCards(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::Scry(_)
        | LocalEffectNode::Surveil(_) => true,
        LocalEffectNode::OptionalLibrarySearch(search)
        | LocalEffectNode::ControllerLibrarySearch(search) => match search.destination {
            SearchDestination::Zone(zone) => match zone {
                CardZoneName::Library => false,
                CardZoneName::Hand
                | CardZoneName::Graveyard
                | CardZoneName::Exile
                | CardZoneName::Battlefield => true,
            },
            SearchDestination::Battlefield { .. } => true,
            // This node selects from and returns to the same player's
            // library; ordering within a zone does not cross its boundary.
            SearchDestination::LibraryTop => false,
        },
        LocalEffectNode::MoveCards(movement) => {
            fn is_library(zone: LocalZone) -> bool {
                match zone {
                    LocalZone::Library => true,
                    LocalZone::Hand => false,
                }
            }
            // Both zones belong to movement.player. A future cross-player
            // library move needs separate source/destination identities.
            is_library(movement.source) != is_library(movement.destination)
        }
        LocalEffectNode::TargetCardZoneChange(movement) => {
            let source_is_library = match movement.source_zone {
                TargetCardSourceZone::AnyGraveyard
                | TargetCardSourceZone::SingleGraveyard
                | TargetCardSourceZone::ControllerGraveyard => false,
            };
            let destination_is_library = match movement.action {
                CardZoneChangeAction::Move(CardZoneDestination::TargetOwnerLibraryTop) => true,
                CardZoneChangeAction::Exile
                | CardZoneChangeAction::Move(CardZoneDestination::ControllerHand) => false,
            };
            source_is_library || destination_is_library
        }
        LocalEffectNode::MoveSource(movement) => match (movement.from, movement.to) {
            (
                SourceZone::Battlefield
                | SourceZone::Exile
                | SourceZone::Graveyard
                | SourceZone::Hand,
                SourceZone::Battlefield
                | SourceZone::Exile
                | SourceZone::Graveyard
                | SourceZone::Hand,
            ) => false,
        },
        LocalEffectNode::OptionalCardDeployment(deployment) => {
            let source_is_library = match deployment.source {
                CardDeploymentSourceZone::Hand | CardDeploymentSourceZone::Graveyard => false,
            };
            let destination_is_library = match deployment.destination {
                CardDeploymentDestination::Battlefield => false,
            };
            source_is_library || destination_is_library
        }
        LocalEffectNode::ZoneChange(movement) => match movement.action {
            ZoneChangeAction::Exile | ZoneChangeAction::Destroy => false,
            ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand) => false,
        },
        LocalEffectNode::ZoneBatch(movement) => match movement.collection {
            ZoneCollectionNode::AllGraveyards | ZoneCollectionNode::OpponentGraveyards => false,
        },
        LocalEffectNode::Retarget(_)
        | LocalEffectNode::CounterSpell(_)
        | LocalEffectNode::TemporaryKeywordGrant(_)
        | LocalEffectNode::AddMana(_)
        | LocalEffectNode::DiscardCards(_)
        | LocalEffectNode::DealDamage(_)
        | LocalEffectNode::Amass(_)
        | LocalEffectNode::EmpowerJace(_)
        | LocalEffectNode::Investigate(_)
        | LocalEffectNode::ControllerKeywordAction(_)
        | LocalEffectNode::Earthbend(_)
        | LocalEffectNode::TransformPermanent(_)
        | LocalEffectNode::PutCounters(_)
        | LocalEffectNode::CreateExplicitTokens(_)
        | LocalEffectNode::CreateTokens(_)
        | LocalEffectNode::CreateNamedTokens(_)
        | LocalEffectNode::GainLife(_)
        | LocalEffectNode::LoseLife(_)
        | LocalEffectNode::TargetPlayerLife(_)
        | LocalEffectNode::LoseAbilities(_)
        | LocalEffectNode::DestroyReferencedSet(_)
        | LocalEffectNode::DestroyCollection(_)
        | LocalEffectNode::Sacrifice(_)
        | LocalEffectNode::ModifyStats(_)
        | LocalEffectNode::BecomeTypes(_)
        | LocalEffectNode::QuantifiedPlayerAction(_)
        | LocalEffectNode::TemporaryPlayerPermission(_)
        | LocalEffectNode::SelectedPermanents(_)
        | LocalEffectNode::TapPermanents(_)
        | LocalEffectNode::PermanentKeywordAction(_)
        | LocalEffectNode::ControlChange(_)
        | LocalEffectNode::CopyStackObject(_)
        | LocalEffectNode::PreventDamage(_)
        | LocalEffectNode::Attach(_)
        | LocalEffectNode::Choose(_)
        | LocalEffectNode::UntapTarget(_)
        | LocalEffectNode::Fight(_) => false,
    }
}

fn costs_move_cards_to_or_from_library(costs: &ActivationCostSequenceNode) -> bool {
    costs.costs.iter().any(|cost| match cost {
        LocalActivationCostNode::PayMana(_)
        | LocalActivationCostNode::TapSelf(_)
        | LocalActivationCostNode::PayLife(_)
        | LocalActivationCostNode::SacrificeSelf(_)
        | LocalActivationCostNode::DiscardSelf(_)
        | LocalActivationCostNode::ExileSelf(_)
        | LocalActivationCostNode::DiscardCard(_)
        | LocalActivationCostNode::TapObjects(_)
        | LocalActivationCostNode::SacrificeObject(_)
        | LocalActivationCostNode::RemoveSourceCounters(_)
        | LocalActivationCostNode::RemoveControlledCounters(_)
        | LocalActivationCostNode::ExileCards(_) => false,
    })
}

/// CR605 qualification facts supplied by typed producers, before IR lowering.
/// Cost choices are not targets of the ability. Producing mana need not be
/// its only action; backend sequencing capabilities are checked separately.
pub(in crate::oracle::canonical) struct ManaAbilityQualificationFacts {
    pub(in crate::oracle::canonical) could_add_mana: bool,
    pub(in crate::oracle::canonical) has_targets: bool,
    pub(in crate::oracle::canonical) is_loyalty: bool,
    pub(in crate::oracle::canonical) effects_cross_library: bool,
    pub(in crate::oracle::canonical) costs_cross_library: bool,
}

impl ManaAbilityQualificationFacts {
    pub(in crate::oracle::canonical) fn qualifies(&self) -> bool {
        self.could_add_mana
            && !self.has_targets
            && !self.is_loyalty
            && !self.effects_cross_library
            && !self.costs_cross_library
    }
}

/// Facts describe execution of this program, not abilities embedded in a
/// created token. One AddMana action can produce a fixed vector or offer a
/// choice; it is still one production action in either case.
pub(in crate::oracle::canonical) struct ManaProgramFacts {
    immediate_mana_actions: usize,
    first_action_produces_mana: bool,
    moves_library: bool,
}

impl ManaProgramFacts {
    pub(in crate::oracle::canonical) fn from_effect_sequence(
        sequence: &EffectSequenceNode,
    ) -> Self {
        Self {
            immediate_mana_actions: sequence
                .effects
                .iter()
                .filter(|effect| produces_mana_while_resolving(effect))
                .count(),
            first_action_produces_mana: sequence
                .effects
                .first()
                .is_some_and(produces_mana_while_resolving),
            moves_library: sequence.effects.iter().any(moves_cards_to_or_from_library),
        }
    }

    /// The caller has already established an activated, non-loyalty
    /// envelope. These remaining criteria are execution facts, not hints.
    pub(in crate::oracle::canonical) fn qualifies_as_mana_ability(
        &self,
        costs: &ActivationCostSequenceNode,
        requires_target: bool,
    ) -> bool {
        ManaAbilityQualificationFacts {
            could_add_mana: self.immediate_mana_actions > 0,
            has_targets: requires_target,
            // This caller's activated envelope excludes loyalty abilities.
            is_loyalty: false,
            effects_cross_library: self.moves_library,
            costs_cross_library: costs_move_cards_to_or_from_library(costs),
        }
        .qualifies()
    }
}

/// The current mana backend produces one selected output before executing
/// non-mana followups. These limits are execution failures, not permission to
/// fall back to a legacy rule that drops or reorders required actions.
pub(in crate::oracle::canonical) fn require_supported_mana_program(
    cursor: &mut OracleTokenCursor<'_>,
    costs: &ActivationCostSequenceNode,
    facts: &ManaProgramFacts,
) -> MachineResult<()> {
    cursor.enter("program.mana.backend-effect-order");
    cursor.require(
        facts.immediate_mana_actions == 1 && facts.first_action_produces_mana,
        "one immediate mana-production action before its non-mana followups; the current mana backend cannot sequence multiple or later production actions",
    )?;
    require_supported_mana_cost_context(cursor, costs)
}

/// Shared backend payment boundary for both sequence programs and specialized
/// conditional mana parents. Producing a declaration does not make variable
/// mana payment or a hand-source mana activation executable in this backend.
pub(in crate::oracle::canonical) fn require_supported_mana_cost_context(
    cursor: &mut OracleTokenCursor<'_>,
    costs: &ActivationCostSequenceNode,
) -> MachineResult<()> {
    cursor.enter("program.mana.backend-cost-context");
    cursor.require(
        costs.costs.iter().all(|cost| {
            !matches!(
                cost,
                LocalActivationCostNode::DiscardSelf(_)
                    | LocalActivationCostNode::ExileSelf(_)
                    // The mana activation backend pays neither chosen card
                    // exile nor counters spread over controlled permanents.
                    | LocalActivationCostNode::ExileCards(_)
                    | LocalActivationCostNode::RemoveControlledCounters(_)
            ) && !matches!(cost, LocalActivationCostNode::PayMana(mana) if mana.contains_variable_x())
        }),
        "battlefield mana costs without an unresolved variable payment; hand-zone, graveyard-zone and variable mana activation support require backend context",
    )
}

#[cfg(test)]
#[path = "mana_program_support_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mana_qualification_tests.rs"]
mod qualification_tests;
