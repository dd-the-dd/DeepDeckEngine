//! A checked player/collection scope slice over the existing program tree.
//! No JSON construction, Oracle reparsing, game-state evaluation or ID allocation.
use super::super::*;

pub(in crate::oracle::canonical) struct AbilityBodyNode {
    pub context: EffectProgramContext,
    pub costs: Option<ActivationCostSequenceNode>,
    pub program: EffectProgramNode,
    pub annotation: Option<ActivationTimingAnnotation>,
}

/// Private payload: checked player/collection producer scopes only. Costs,
/// numeric support and IR capability validation retain their native owners.
pub(in crate::oracle::canonical) struct ScopeCheckedAbilityBody(AbilityBodyNode);

impl ScopeCheckedAbilityBody {
    pub(in crate::oracle::canonical) fn into_body(self) -> AbilityBodyNode {
        self.0
    }
}

pub(in crate::oracle::canonical) enum AbilityBodyResolution {
    ScopeChecked(ScopeCheckedAbilityBody),
    /// Structural classification only; no failed semantic check may reach here.
    OutsideSlice(AbilityBodyNode),
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ProgramResolutionError {
    pub state: &'static str,
    pub expectation: &'static str,
    pub support_failure: bool,
}

fn error(state: &'static str, expectation: &'static str) -> ProgramResolutionError {
    ProgramResolutionError {
        state,
        expectation,
        support_failure: state == "lower.card-action.bindings",
    }
}

fn independent_number(amount: &NumericExpression) -> bool {
    match amount {
        NumericExpression::Atom(NumericAtom::Integer(_)) => true,
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            independent_number(left) && independent_number(right)
        }
        NumericExpression::Atom(NumericAtom::VariableX)
        | NumericExpression::ObjectProperty { .. }
        | NumericExpression::PowerOf(_)
        | NumericExpression::ManaValueOf(_)
        | NumericExpression::QualifiedTargetProperty { .. }
        | NumericExpression::CountQuery(_)
        | NumericExpression::CountCounters { .. }
        | NumericExpression::PermanentCount(_)
        | NumericExpression::SourceCastX
        | NumericExpression::EventValue(_)
        | NumericExpression::CountDistinctCardTypes { .. }
        | NumericExpression::PlayerFact { .. }
        | NumericExpression::SacrificedCostObjectProperty(_)
        | NumericExpression::GreatestPropertyAmong { .. }
        | NumericExpression::CardsInZone { .. } => false,
    }
}

fn simple_operand(operand: &CardOperandNode) -> bool {
    match operand {
        CardOperandNode::Reference(reference) => matches!(
            reference,
            CardCollectionReferenceNode::Antecedent | CardCollectionReferenceNode::Selected
        ),
        CardOperandNode::Selection(selection) => {
            selection.predicate.is_none()
                && selection.from == CardCollectionReferenceNode::Antecedent
                && match &selection.quantity {
                    CardSelectionQuantityNode::Exactly(amount)
                    | CardSelectionQuantityNode::UpTo(amount) => independent_number(amount),
                    CardSelectionQuantityNode::AnyNumber => true,
                    CardSelectionQuantityNode::All { .. } => false,
                }
        }
        CardOperandNode::ZoneCollection(_)
        | CardOperandNode::Targets(_)
        | CardOperandNode::LibraryTop(_) => false,
    }
}

/// Closed classification happens before validation. A new effect variant must
/// explicitly join this slice or retain the existing native lowering boundary.
fn in_slice(sequence: &EffectSequenceNode, nested: bool) -> bool {
    // Shared optional observation/export validation still belongs to the native
    // collection lowerer. Do not move its rejection point into a partial visitor.
    if sequence
        .effects
        .iter()
        .any(|effect| matches!(effect, LocalEffectNode::CardAction(_)))
        && sequence
            .effects
            .iter()
            .any(|effect| matches!(effect, LocalEffectNode::OptionalSequence(_)))
    {
        return false;
    }
    sequence.effects.iter().all(|effect| match effect {
        LocalEffectNode::PlayerAction(node) => match &node.action {
            SingularPlayerAction::Draw(amount) | SingularPlayerAction::Mill(amount) => {
                independent_number(amount)
            }
            SingularPlayerAction::Life(life) => independent_number(&life.amount),
            // Choice-bearing player actions keep the native lowering boundary.
            SingularPlayerAction::Discard(_)
            | SingularPlayerAction::Sacrifice(_)
            | SingularPlayerAction::Venture(_) => false,
        },
        LocalEffectNode::TargetPlayerLife(node) => independent_number(&node.amount),
        LocalEffectNode::GainLife(node) => {
            node.player == LocalPlayerReference::Controller && independent_number(&node.amount)
        }
        LocalEffectNode::LoseLife(node) => {
            node.player == LocalPlayerReference::Controller && independent_number(&node.amount)
        }
        LocalEffectNode::DrawCards(node) => {
            node.player == LocalPlayerReference::Controller && independent_number(&node.count)
        }
        LocalEffectNode::Scry(node) => {
            node.player == LocalPlayerReference::Controller && independent_number(&node.count)
        }
        LocalEffectNode::Surveil(node) => {
            node.player == LocalPlayerReference::Controller && independent_number(&node.count)
        }
        LocalEffectNode::CardAction(action) if !nested => match action {
            CardActionNode::Look { owner, count } => {
                *owner == LocalPlayerReference::Controller && independent_number(count)
            }
            CardActionNode::Reveal(operand) => simple_operand(operand),
            // A permission window is lowered by the native collection lowerer.
            CardActionNode::Permit(_)
            | CardActionNode::RevealUntil { .. }
            | CardActionNode::Shuffle { .. } => false,
            CardActionNode::Move { cards, destination } => {
                simple_operand(cards)
                    && match destination.owner {
                        CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
                        | CardMoveOwnerNode::EachCardOwner => true,
                        CardMoveOwnerNode::Player(
                            LocalPlayerReference::ControllerOf(_)
                            | LocalPlayerReference::ControllerOfQualified(_),
                        ) => false,
                    }
            }
        },
        // Optional collection exports retain their existing native checks until
        // their dominance/export rules migrate; they are never marked checked.
        LocalEffectNode::CardAction(_) => false,
        LocalEffectNode::OptionalCardDeployment(deployment) => {
            deployment.player == LocalPlayerReference::Controller
        }
        LocalEffectNode::OptionalSequence(body) => in_slice(body, true),
        LocalEffectNode::QuantifiedPlayerAction(_)
        | LocalEffectNode::CounterSpell(_)
        | LocalEffectNode::Retarget(_)
        | LocalEffectNode::TemporaryKeywordGrant(_)
        | LocalEffectNode::AddMana(_)
        | LocalEffectNode::DealDamage(_)
        | LocalEffectNode::DiscardCards(_)
        | LocalEffectNode::OptionalDrawCards(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::AnimateSource(_)
        | LocalEffectNode::Amass(_)
        | LocalEffectNode::EmpowerJace(_)
        | LocalEffectNode::Investigate(_)
        | LocalEffectNode::Earthbend(_)
        | LocalEffectNode::TransformPermanent(_)
        | LocalEffectNode::PutCounters(_)
        | LocalEffectNode::MoveSource(_)
        | LocalEffectNode::MoveCards(_)
        | LocalEffectNode::ZoneBatch(_)
        | LocalEffectNode::ZoneChange(_)
        | LocalEffectNode::TargetCardZoneChange(_)
        | LocalEffectNode::CreateTokens(_)
        | LocalEffectNode::CreateExplicitTokens(_)
        | LocalEffectNode::CreateNamedTokens(_)
        | LocalEffectNode::OptionalLibrarySearch(_)
        | LocalEffectNode::ControllerLibrarySearch(_)
        | LocalEffectNode::LoseAbilities(_)
        | LocalEffectNode::DestroyReferencedSet(_)
        | LocalEffectNode::DestroyCollection(_)
        | LocalEffectNode::Sacrifice(_)
        | LocalEffectNode::ModifyStats(_)
        | LocalEffectNode::BecomeTypes(_)
        | LocalEffectNode::UntapTarget(_)
        | LocalEffectNode::SelectedPermanents(_)
        | LocalEffectNode::TapPermanents(_)
        | LocalEffectNode::PermanentKeywordAction(_)
        | LocalEffectNode::PreventDamage(_)
        | LocalEffectNode::Attach(_)
        | LocalEffectNode::Choose(_)
        | LocalEffectNode::TemporaryPlayerPermission(_) => false,
    })
}

fn program_in_slice(program: &EffectProgramNode) -> bool {
    match program {
        EffectProgramNode::Sequence(sequence) => in_slice(sequence, false),
        EffectProgramNode::Optional { player, body }
        | EffectProgramNode::OptionalCost { player, body, .. } => {
            *player == LocalPlayerReference::Controller && program_in_slice(body)
        }
        // The checked scope walks one sequence; independent steps and the
        // two sentences of an optional action keep the native lowering checks.
        EffectProgramNode::Steps(_)
        | EffectProgramNode::OptionalAction { .. }
        | EffectProgramNode::ReflexiveAction { .. } => false,
    }
}

/// This scope records producer facts only, not runtime object IDs or decisions.
#[derive(Default)]
struct CollectionScope {
    observation: bool,
    singular: Option<Option<i64>>,
    plural: Option<Option<i64>>,
}

impl CollectionScope {
    fn mention(&mut self, maximum: Option<i64>) {
        if maximum.is_some_and(|maximum| maximum <= 1) {
            self.singular = Some(maximum);
        } else {
            self.singular = None;
            self.plural = Some(maximum);
        }
    }

    fn reference(
        &self,
        reference: CardCollectionReferenceNode,
    ) -> Result<Option<i64>, ProgramResolutionError> {
        const STATE: &str = "lower.card-action.bindings";
        if !self.observation {
            return Err(error(
                STATE,
                "an observed card collection in the current lexical scope",
            ));
        }
        match reference {
            CardCollectionReferenceNode::Antecedent => self.plural.ok_or_else(|| {
                error(
                    STATE,
                    "a plural card-collection antecedent in the current scope",
                )
            }),
            CardCollectionReferenceNode::Selected => self
                .singular
                .ok_or_else(|| error(STATE, "a singular card antecedent in the current scope")),
            CardCollectionReferenceNode::Remainder | CardCollectionReferenceNode::Other => {
                unreachable!("structurally outside the checked scope slice")
            }
        }
    }

    fn operand(&mut self, operand: &CardOperandNode) -> Result<(), ProgramResolutionError> {
        match operand {
            CardOperandNode::Reference(reference) => {
                let maximum = self.reference(*reference)?;
                self.mention(maximum);
                Ok(())
            }
            CardOperandNode::Selection(selection) => {
                self.reference(selection.from)?;
                let maximum = match &selection.quantity {
                    CardSelectionQuantityNode::Exactly(amount)
                    | CardSelectionQuantityNode::UpTo(amount) => literal(amount),
                    CardSelectionQuantityNode::AnyNumber
                    | CardSelectionQuantityNode::All { .. } => None,
                };
                self.mention(maximum);
                Ok(())
            }
            CardOperandNode::ZoneCollection(_)
            | CardOperandNode::Targets(_)
            | CardOperandNode::LibraryTop(_) => {
                unreachable!("outside the checked scope slice")
            }
        }
    }
}

fn literal(amount: &NumericExpression) -> Option<i64> {
    match amount {
        NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
        _ => None,
    }
}

fn resolve_sequence(
    sequence: &EffectSequenceNode,
    collections: &mut CollectionScope,
) -> Result<(), ProgramResolutionError> {
    let mut players = std::collections::BTreeSet::<usize>::new();
    for effect in &sequence.effects {
        match effect {
            LocalEffectNode::PlayerAction(action) => {
                match &action.player {
                    PlayerEffectSubject::Controller
                    | PlayerEffectSubject::EventPlayer
                    | PlayerEffectSubject::ObjectRelation { .. } => {}
                    PlayerEffectSubject::DeclaredTarget { binding, target } => {
                        let [slot] = target.slots.as_slice() else {
                            return Err(error(
                                "lower.player-action.subject",
                                "a valid declared or previously bound singular player subject",
                            ));
                        };
                        if !matches!(slot.selector, TargetSelector::Player(_))
                            || slot.cardinality != TargetCardinality::EXACTLY_ONE
                        {
                            return Err(error(
                                "lower.player-action.subject",
                                "a valid declared or previously bound singular player subject",
                            ));
                        }
                        players.insert(binding.0);
                    }
                    PlayerEffectSubject::Bound(binding) if !players.contains(&binding.0) => {
                        return Err(error(
                            "lower.player-action.subject",
                            "an earlier declared player subject target slot",
                        ));
                    }
                    PlayerEffectSubject::Bound(_) => {}
                }
                if matches!(
                    action.action,
                    SingularPlayerAction::Draw(_) | SingularPlayerAction::Mill(_)
                ) {
                    *collections = CollectionScope::default();
                }
            }
            LocalEffectNode::TargetPlayerLife(life) => {
                if life.declares_target {
                    players.insert(life.player.effect_index);
                }
                if !players.contains(&life.player.effect_index) {
                    return Err(error(
                        "lower.target-player-life.binding",
                        "an earlier declared player target binding",
                    ));
                }
            }
            LocalEffectNode::CardAction(action) => match action {
                CardActionNode::Look { count, .. } => {
                    *collections = CollectionScope {
                        observation: true,
                        singular: None,
                        plural: Some(literal(count)),
                    };
                    collections.mention(literal(count));
                }
                CardActionNode::Reveal(operand) => collections.operand(operand)?,
                CardActionNode::Move { cards, .. } => collections.operand(cards)?,
                CardActionNode::Permit(_)
                | CardActionNode::RevealUntil { .. }
                | CardActionNode::Shuffle { .. } => {
                    unreachable!("a permission window is outside the checked scope slice")
                }
            },
            // Nested sequence declarations neither borrow nor escape player
            // scope. The existing lowerer has an independent player registry.
            LocalEffectNode::OptionalSequence(body) => resolve_sequence(body, collections)?,
            LocalEffectNode::OptionalCardDeployment(_) => {
                *collections = CollectionScope::default();
            }
            LocalEffectNode::DrawCards(_)
            | LocalEffectNode::Scry(_)
            | LocalEffectNode::Surveil(_) => {
                *collections = CollectionScope::default();
            }
            LocalEffectNode::GainLife(_) | LocalEffectNode::LoseLife(_) => {}
            // Closed structural classification has already excluded these.
            LocalEffectNode::QuantifiedPlayerAction(_)
            | LocalEffectNode::CounterSpell(_)
            | LocalEffectNode::Retarget(_)
            | LocalEffectNode::TemporaryKeywordGrant(_)
            | LocalEffectNode::AddMana(_)
            | LocalEffectNode::DealDamage(_)
            | LocalEffectNode::DiscardCards(_)
            | LocalEffectNode::OptionalDrawCards(_)
            | LocalEffectNode::MillCards(_)
            | LocalEffectNode::AnimateSource(_)
            | LocalEffectNode::Amass(_)
            | LocalEffectNode::EmpowerJace(_)
            | LocalEffectNode::Investigate(_)
            | LocalEffectNode::Earthbend(_)
            | LocalEffectNode::TransformPermanent(_)
            | LocalEffectNode::PutCounters(_)
            | LocalEffectNode::MoveSource(_)
            | LocalEffectNode::MoveCards(_)
            | LocalEffectNode::ZoneBatch(_)
            | LocalEffectNode::ZoneChange(_)
            | LocalEffectNode::TargetCardZoneChange(_)
            | LocalEffectNode::CreateTokens(_)
            | LocalEffectNode::CreateExplicitTokens(_)
            | LocalEffectNode::CreateNamedTokens(_)
            | LocalEffectNode::OptionalLibrarySearch(_)
            | LocalEffectNode::ControllerLibrarySearch(_)
            | LocalEffectNode::LoseAbilities(_)
            | LocalEffectNode::DestroyReferencedSet(_)
            | LocalEffectNode::DestroyCollection(_)
            | LocalEffectNode::Sacrifice(_)
            | LocalEffectNode::ModifyStats(_)
            | LocalEffectNode::BecomeTypes(_)
            | LocalEffectNode::UntapTarget(_)
            | LocalEffectNode::SelectedPermanents(_)
            | LocalEffectNode::TapPermanents(_)
            | LocalEffectNode::PermanentKeywordAction(_)
            | LocalEffectNode::PreventDamage(_)
            | LocalEffectNode::Attach(_)
            | LocalEffectNode::Choose(_)
            | LocalEffectNode::TemporaryPlayerPermission(_) => {
                unreachable!("outside the checked scope slice")
            }
        }
    }
    Ok(())
}

pub(in crate::oracle::canonical) fn resolve_ability_body_scopes(
    body: AbilityBodyNode,
) -> Result<AbilityBodyResolution, ProgramResolutionError> {
    // The common body envelope retains costs, but this first scope-checked
    // adoption is ordinary spells only; cost references have not migrated.
    if body.context != EffectProgramContext::Spell
        || body.costs.is_some()
        || !program_in_slice(&body.program)
    {
        return Ok(AbilityBodyResolution::OutsideSlice(body));
    }
    resolve_sequence(
        body.program.inner_sequence(),
        &mut CollectionScope::default(),
    )?;
    Ok(AbilityBodyResolution::ScopeChecked(
        ScopeCheckedAbilityBody(body),
    ))
}

#[cfg(test)]
#[path = "program_resolution_tests.rs"]
mod tests;
