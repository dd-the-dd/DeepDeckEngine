//! A checked player/collection scope slice over the existing program tree.
//! No JSON construction, Oracle reparsing, game-state evaluation or ID allocation.
//!
//! It also resolves action-result objects (CR 608.2c) over a lowered program:
//! the reader of the one object an earlier action selected or moved is bound
//! to the binding that action records. That resolution only rewrites lowered
//! references and installs the producer's compiler-assigned `bind`.
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
        | NumericExpression::CountersOnRecipient { .. }
        | NumericExpression::PermanentCount(_)
        | NumericExpression::SourceCastX
        | NumericExpression::ColorsOfManaSpentToCastSource
        | NumericExpression::EventValue(_)
        | NumericExpression::CountDistinctCardTypes { .. }
        | NumericExpression::OpponentCount(_)
        | NumericExpression::EventHistoryCount(_)
        | NumericExpression::Devotion(_)
        | NumericExpression::ActionResultQuantity { .. }
        | NumericExpression::PlayerFact { .. }
        | NumericExpression::SacrificedCostObjectProperty(_)
        | NumericExpression::BoundObjectProperty { .. }
        | NumericExpression::Quantity(_)
        | NumericExpression::GreatestPropertyAmong { .. }
        | NumericExpression::ActionResultCount { .. }
        | NumericExpression::CardsInZone { .. } => false,
    }
}

fn simple_operand(operand: &CardOperandNode) -> bool {
    match operand {
        CardOperandNode::Reference(reference)
        | CardOperandNode::ObjectAntecedent { reference, .. } => matches!(
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
        | CardOperandNode::LibraryTop(_)
        | CardOperandNode::CardCopies { .. }
        | CardOperandNode::HiddenChoice(_)
        | CardOperandNode::CommandZone { .. }
        // The restated query is proved against the antecedent's filter by
        // the native collection lowerer.
        | CardOperandNode::RestatedAntecedent { .. } => false,
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
            SingularPlayerAction::SetLifeTotal(total) => independent_number(total),
            // Choice-bearing player actions keep the native lowering boundary.
            // A revealed hand and a referenced discard bind card collections.
            SingularPlayerAction::Counters(node) => independent_number(&node.count),
            SingularPlayerAction::CreateTokens(_) | SingularPlayerAction::Scry(_) => false,
            SingularPlayerAction::Discard(_)
            | SingularPlayerAction::Sacrifice(_)
            | SingularPlayerAction::Venture(_)
            | SingularPlayerAction::Proliferate(_)
            | SingularPlayerAction::Hand(_)
            | SingularPlayerAction::ExtraTurn
            | SingularPlayerAction::Designation(_)
            | SingularPlayerAction::Skip(_)
            | SingularPlayerAction::Prohibition(_)
            | SingularPlayerAction::Status(_)
            | SingularPlayerAction::ReferencedCards { .. } => false,
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
            | CardActionNode::Copy { .. }
            | CardActionNode::LookAt(_)
            | CardActionNode::RevealUntil { .. }
            | CardActionNode::Choose(_)
            | CardActionNode::ChooseFromZone(_)
            | CardActionNode::GrantKeyword(_)
            | CardActionNode::ChooseTargetCards(_)
            | CardActionNode::Shuffle { .. } => false,
            CardActionNode::Move { cards, destination } => {
                simple_operand(cards)
                    && match destination.owner {
                        CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
                        | CardMoveOwnerNode::EachCardOwner => true,
                        CardMoveOwnerNode::Player(
                            LocalPlayerReference::ControllerOf(_)
                            | LocalPlayerReference::ControllerOfQualified(_)
                            | LocalPlayerReference::Bound(_),
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
        | LocalEffectNode::Amass(_)
        | LocalEffectNode::EmpowerJace(_)
        | LocalEffectNode::Investigate(_)
        | LocalEffectNode::ControllerKeywordAction(_)
        | LocalEffectNode::Earthbend(_)
        | LocalEffectNode::TransformPermanent(_)
        | LocalEffectNode::PutCounters(_)
        | LocalEffectNode::MoveCounters(_)
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
        | LocalEffectNode::ReferencedPermanentAction(_)
        | LocalEffectNode::UntapTarget(_)
        | LocalEffectNode::Fight(_)
        | LocalEffectNode::SelectedPermanents(_)
        | LocalEffectNode::TapPermanents(_)
        | LocalEffectNode::UntapRestriction(_)
        | LocalEffectNode::RuleChange(_)
        | LocalEffectNode::Performed(_)
        | LocalEffectNode::PermanentKeywordAction(_)
        | LocalEffectNode::ControlChange(_)
        | LocalEffectNode::CopyStackObject(_)
        | LocalEffectNode::AdditionalPhases(_)
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
        | EffectProgramNode::Outcome {
            permission: Some(player),
            producer: OutcomeProducer::Cost(_),
            body,
            otherwise: None,
            ..
        } => *player == LocalPlayerReference::Controller && program_in_slice(body),
        // The checked scope walks one sequence; independent steps and the
        // two sentences of an outcome action keep the native lowering checks.
        EffectProgramNode::Steps(_)
        | EffectProgramNode::Outcome { .. }
        | EffectProgramNode::Alternatives { .. }
        | EffectProgramNode::Offer { .. }
        | EffectProgramNode::CreatedReplacement { .. }
        | EffectProgramNode::OncePerTurn { .. } => false,
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
        // A linked set is not an observation of this program (CR 607.2a).
        if reference == CardCollectionReferenceNode::Linked {
            return Ok(None);
        }
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
            CardCollectionReferenceNode::Remainder
            | CardCollectionReferenceNode::Other
            | CardCollectionReferenceNode::Zone
            | CardCollectionReferenceNode::Exiled
            | CardCollectionReferenceNode::Linked
            | CardCollectionReferenceNode::Revealed => {
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
            // The observed collection is the nearer antecedent of a card
            // pronoun; without one the pronoun names its typed object (one
            // object for `it` / `that card`, a set for `them`).
            CardOperandNode::ObjectAntecedent { reference, .. } => {
                let maximum = if self.observation {
                    self.reference(*reference)?
                } else {
                    match reference {
                        CardCollectionReferenceNode::Selected => Some(1),
                        _ => None,
                    }
                };
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
            | CardOperandNode::LibraryTop(_)
            | CardOperandNode::CardCopies { .. }
            | CardOperandNode::HiddenChoice(_)
            | CardOperandNode::CommandZone { .. }
            | CardOperandNode::RestatedAntecedent { .. } => {
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
                    SingularPlayerAction::Draw(_)
                        | SingularPlayerAction::Mill(_)
                        | SingularPlayerAction::Scry(_)
                        | SingularPlayerAction::CreateTokens(_)
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
                | CardActionNode::Copy { .. }
                | CardActionNode::LookAt(_)
                | CardActionNode::RevealUntil { .. }
                | CardActionNode::Choose(_)
                | CardActionNode::ChooseFromZone(_)
                | CardActionNode::GrantKeyword(_)
                | CardActionNode::ChooseTargetCards(_)
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
            | LocalEffectNode::Amass(_)
            | LocalEffectNode::EmpowerJace(_)
            | LocalEffectNode::Investigate(_)
            | LocalEffectNode::ControllerKeywordAction(_)
            | LocalEffectNode::Earthbend(_)
            | LocalEffectNode::TransformPermanent(_)
            | LocalEffectNode::PutCounters(_)
            | LocalEffectNode::MoveCounters(_)
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
            | LocalEffectNode::ReferencedPermanentAction(_)
            | LocalEffectNode::UntapTarget(_)
            | LocalEffectNode::Fight(_)
            | LocalEffectNode::SelectedPermanents(_)
            | LocalEffectNode::TapPermanents(_)
            | LocalEffectNode::UntapRestriction(_)
            | LocalEffectNode::RuleChange(_)
            | LocalEffectNode::Performed(_)
            | LocalEffectNode::PermanentKeywordAction(_)
            | LocalEffectNode::ControlChange(_)
            | LocalEffectNode::CopyStackObject(_)
            | LocalEffectNode::AdditionalPhases(_)
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

/// How the lowered program names the one object an action of the sequence
/// selected or moved on resolution (`automata::action_result_object`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionResultObjectPlan {
    /// The producing action already names its object by this binding (a
    /// recording action's own `bind`, the stop card of an exile-until, the
    /// bound card set a reveal or a move acts on).
    Bound(String),
    /// The one recording action of the producer carries the pending marker:
    /// a reader installs its `bind` (`actionResult:N`).
    Marked,
    /// The producer's object is the result of this resolution choice (a
    /// card chosen at random and revealed): a numeric property read names
    /// it by this id, any other reader as the decision's result.
    Decision(String),
}

/// The plans of one lowered sequence, by producing action.
pub(in crate::oracle::canonical) type ActionResultObjectPlans =
    std::collections::BTreeMap<usize, ActionResultObjectPlan>;

/// Marks the runtime action that records a producer's result until a reader
/// of that result binds it; every marker is removed before the program is
/// returned.
const ACTION_RESULT_MARKER: &str = "pendingActionResultObject";

/// Runtime actions whose `bind` records exactly the objects they selected
/// or moved: sacrificed, destroyed, discarded, milled and exiled ones.
const RECORDING_KINDS: [&str; 6] = [
    "sacrificePermanents",
    "sacrificePermanent",
    "destroyPermanent",
    "discardCards",
    "mill",
    "exileTargetCards",
];

/// The binding of a stable bound object set (`boundObjects`,
/// `singleBoundObject`, `boundObject`), if `value` is one.
fn stable_binding(value: &Value) -> Option<String> {
    matches!(
        value.get("kind").and_then(Value::as_str),
        Some("boundObjects" | "singleBoundObject" | "boundObject")
    )
    .then(|| value.get("binding").and_then(Value::as_str))
    .flatten()
    .map(str::to_owned)
}

/// Every lowered action under `value` that puts cards onto the battlefield,
/// at any depth (a manifest's face-down move inside its choice frame).
fn battlefield_moves<'a>(value: &'a mut Value, found: &mut Vec<&'a mut Value>) {
    let enters = value.get("kind").and_then(Value::as_str) == Some("moveCards")
        && value
            .get("to")
            .and_then(|to| to.get("kind"))
            .and_then(Value::as_str)
            == Some("battlefield");
    if enters {
        found.push(value);
        return;
    }
    match value {
        Value::Object(map) => map
            .values_mut()
            .for_each(|child| battlefield_moves(child, found)),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| battlefield_moves(child, found)),
        _ => {}
    }
}

/// The plan of a producer whose object is a permanent it put onto the
/// battlefield (a manifested card, CR 701.40a, 701.62a): the one move that
/// enters it records it (its `bind`, else a marker).
fn battlefield_producer_plan(
    lowered: &mut [Value],
    effect_index: usize,
) -> Option<ActionResultObjectPlan> {
    let mut moves = Vec::new();
    lowered
        .iter_mut()
        .for_each(|effect| battlefield_moves(effect, &mut moves));
    let [producer] = moves.as_mut_slice() else {
        return None;
    };
    Some(match producer.get("bind").and_then(Value::as_str) {
        Some(existing) => ActionResultObjectPlan::Bound(existing.to_owned()),
        None => {
            producer[ACTION_RESULT_MARKER] = json!(effect_index);
            ActionResultObjectPlan::Marked
        }
    })
}

/// The plan of one producer from its lowered top-level actions: its one
/// recording action (its `bind`, else a marker); else the stop card of an
/// exile-until; else the one bound card set (or the one chosen card) its
/// actions read. Anything else has no plan, and a reader of its result
/// fails explicitly.
fn producer_plan(lowered: &mut [Value], effect_index: usize) -> Option<ActionResultObjectPlan> {
    let recording = (0..lowered.len())
        .filter(|position| {
            lowered[*position]
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| RECORDING_KINDS.contains(&kind))
        })
        .collect::<Vec<_>>();
    match recording.as_slice() {
        [position] => {
            let producer = &mut lowered[*position];
            return Some(match producer.get("bind").and_then(Value::as_str) {
                Some(existing) => ActionResultObjectPlan::Bound(existing.to_owned()),
                None => {
                    producer[ACTION_RESULT_MARKER] = json!(effect_index);
                    ActionResultObjectPlan::Marked
                }
            });
        }
        [] => {}
        _ => return None,
    }
    if let Some(stop) = lowered
        .iter()
        .find_map(|effect| effect.get("stopCardBind").and_then(Value::as_str))
    {
        return Some(ActionResultObjectPlan::Bound(stop.to_owned()));
    }
    let mut bound: Option<ActionResultObjectPlan> = None;
    for effect in lowered.iter() {
        for field in ["cards", "objects"] {
            let Some(operand) = effect.get(field) else {
                continue;
            };
            let plan = match stable_binding(operand) {
                Some(binding) => ActionResultObjectPlan::Bound(binding),
                None if operand.get("kind").and_then(Value::as_str) == Some("decisionResult") => {
                    let Some(decision) = operand.get("decisionId").and_then(Value::as_str) else {
                        continue;
                    };
                    ActionResultObjectPlan::Decision(decision.to_owned())
                }
                None => continue,
            };
            match &bound {
                None => bound = Some(plan),
                Some(existing) if *existing == plan => {}
                Some(_) => return None,
            }
        }
    }
    bound
}

/// Records, where the program's top-level actions still have their written
/// positions, how each action that selected or moved exactly one object
/// names it (CR 608.2c). Only a reader's binding later makes the plan
/// observable (`resolve_action_result_objects`); an unread plan changes
/// nothing.
pub(in crate::oracle::canonical) fn plan_action_result_objects(
    sequence: &EffectSequenceNode,
    action_starts: &[(usize, usize)],
    effects: &mut [Value],
) -> ActionResultObjectPlans {
    let mut plans = ActionResultObjectPlans::new();
    for (effect_index, effect) in sequence.effects.iter().enumerate() {
        let Some(result) = action_result_object(effect, effect_index) else {
            continue;
        };
        let (Some(&(from, _)), Some(&(to, _))) = (
            action_starts.get(effect_index),
            action_starts.get(effect_index + 1),
        ) else {
            continue;
        };
        if from > to || to > effects.len() {
            continue;
        }
        let lowered = &mut effects[from..to];
        let plan = if result.reference.permanent {
            battlefield_producer_plan(lowered, effect_index)
        } else {
            producer_plan(lowered, effect_index)
        };
        if let Some(plan) = plan {
            plans.insert(effect_index, plan);
        }
    }
    plans
}

/// The producing action of every action-result reader under `value`.
fn collect_action_result_readers(value: &Value, found: &mut Vec<usize>) {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("singleBoundObject")
                && let Some(index) = map
                    .get("binding")
                    .and_then(Value::as_str)
                    .and_then(|binding| binding.strip_prefix("actionResult:"))
                    .and_then(|index| index.parse::<usize>().ok())
                && !found.contains(&index)
            {
                found.push(index);
            }
            map.values()
                .for_each(|child| collect_action_result_readers(child, found));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|child| collect_action_result_readers(child, found)),
        _ => {}
    }
}

/// Installs the `bind` of the marked recording action of `effect_index`,
/// or reads the one it already has.
fn bind_marked_producer(value: &mut Value, effect_index: usize, found: &mut Option<String>) {
    match value {
        Value::Object(map) => {
            if map
                .get(ACTION_RESULT_MARKER)
                .and_then(Value::as_u64)
                .is_some_and(|marked| marked == effect_index as u64)
            {
                let binding = match map.get("bind").and_then(Value::as_str) {
                    Some(existing) => existing.to_owned(),
                    None => {
                        let binding = action_result_binding(effect_index);
                        map.insert("bind".to_owned(), json!(binding));
                        binding
                    }
                };
                *found = Some(binding);
            }
            map.values_mut()
                .for_each(|child| bind_marked_producer(child, effect_index, found));
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| bind_marked_producer(child, effect_index, found)),
        _ => {}
    }
}

/// True when `value` is a reader of the pending result `pending`.
fn reads_pending(value: &Value, pending: &str) -> bool {
    value.get("kind").and_then(Value::as_str) == Some("singleBoundObject")
        && value.get("binding").and_then(Value::as_str) == Some(pending)
}

/// Replaces every reader of `pending`: the object of a numeric property
/// read (`powerOf`, `toughnessOf`, `manaValueOf`) by `numeric`, any other
/// reader by `object`.
fn rewrite_action_result_reader(value: &mut Value, pending: &str, numeric: &Value, object: &Value) {
    if reads_pending(value, pending) {
        *value = object.clone();
        return;
    }
    let property_read = matches!(
        value.get("kind").and_then(Value::as_str),
        Some("powerOf" | "toughnessOf" | "manaValueOf")
    );
    if property_read
        && value
            .get("object")
            .is_some_and(|operand| reads_pending(operand, pending))
    {
        value["object"] = numeric.clone();
    }
    match value {
        Value::Object(map) => map
            .values_mut()
            .for_each(|child| rewrite_action_result_reader(child, pending, numeric, object)),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| rewrite_action_result_reader(child, pending, numeric, object)),
        _ => {}
    }
}

fn clear_action_result_markers(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove(ACTION_RESULT_MARKER);
            map.values_mut().for_each(clear_action_result_markers);
        }
        Value::Array(items) => items.iter_mut().for_each(clear_action_result_markers),
        _ => {}
    }
}

/// Binds every reader of an action result (`that creature's toughness`
/// after `sacrifices a creature of their choice`, CR 608.2c) to the object
/// its producing action recorded, once the whole program is lowered (a
/// condition or a `where X` definition is placed after the producer was
/// planned). A reader whose producer has no plan fails explicitly; the
/// markers of unread plans are removed.
pub(in crate::oracle::canonical) fn resolve_action_result_objects(
    plans: &ActionResultObjectPlans,
    effects: &mut [Value],
) -> Result<(), &'static str> {
    let mut readers = Vec::new();
    effects
        .iter()
        .for_each(|effect| collect_action_result_readers(effect, &mut readers));
    for effect_index in readers {
        let plan = plans.get(&effect_index).ok_or(
            "an earlier action of this program whose lowering records the one object it selected or moved",
        )?;
        let single = |binding: &str| json!({ "kind": "singleBoundObject", "binding": binding });
        let (numeric, object) = match plan {
            ActionResultObjectPlan::Bound(binding) => {
                (single(binding.as_str()), single(binding.as_str()))
            }
            ActionResultObjectPlan::Marked => {
                let mut found = None;
                effects
                    .iter_mut()
                    .for_each(|effect| bind_marked_producer(effect, effect_index, &mut found));
                let binding = found.ok_or("the lowered action that recorded the object")?;
                (single(binding.as_str()), single(binding.as_str()))
            }
            // The engine reads a numeric property of the chosen card from
            // the decision itself (`bound_object_numeric_read`).
            ActionResultObjectPlan::Decision(decision) => (
                single(decision.as_str()),
                json!({ "kind": "decisionResult", "decisionId": decision }),
            ),
        };
        let pending = action_result_binding(effect_index);
        if object != single(pending.as_str()) || numeric != single(pending.as_str()) {
            effects.iter_mut().for_each(|effect| {
                rewrite_action_result_reader(effect, &pending, &numeric, &object)
            });
        }
    }
    effects.iter_mut().for_each(clear_action_result_markers);
    Ok(())
}

#[cfg(test)]
#[path = "program_resolution_tests.rs"]
mod tests;
