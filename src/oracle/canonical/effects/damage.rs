use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn effect_subject_end(input: &AbilityInput<'_>) -> Option<usize> {
    let tokens = input.tokens;
    input
        .name_links
        .iter()
        .find(|link| link.token_start == 0)
        .map(|link| link.token_end)
        .or_else(|| {
            matches!(tokens, [first, second, ..]
                if first.raw.eq_ignore_ascii_case("this")
                    && second.raw.eq_ignore_ascii_case("spell"))
            .then_some(2)
        })
        .or_else(|| {
            input.name_resolution_failure.and_then(|failure| {
                failure
                    .unresolved
                    .iter()
                    .find(|unresolved| unresolved.token_start == 0)
                    .map(|unresolved| unresolved.token_end)
            })
        })
}

fn starts_with_effect_subject(input: &AbilityInput<'_>) -> bool {
    effect_subject_end(input).is_some()
}

pub(in crate::oracle::canonical) fn claims_promoted_damage_life_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    starts_with_effect_subject(input)
        && tokens.windows(3).any(|window| {
            matches!(window[0].raw.to_ascii_lowercase().as_str(), "and" | "then")
                && window[1].raw.eq_ignore_ascii_case("you")
                && window[2].raw.eq_ignore_ascii_case("gain")
        })
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("deals"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("damage"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("life"))
}

pub(in crate::oracle::canonical) fn claims_promoted_conditional_damage_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(condition_start) = tokens.windows(5).position(|window| {
        window[0].raw == "."
            && window[1].raw.eq_ignore_ascii_case("if")
            && window[2].raw.eq_ignore_ascii_case("this")
            && window[3].raw.eq_ignore_ascii_case("spell")
            && window[4].raw.eq_ignore_ascii_case("was")
    }) else {
        return false;
    };
    let (antecedent, replacement) = tokens.split_at(condition_start);

    starts_with_effect_subject(input)
        && antecedent
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("deals"))
        && antecedent
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("damage"))
        && replacement
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("deals"))
        && replacement
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("damage"))
        && replacement
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("instead"))
}

/// Claims the direct single-sentence damage grammar at its promoted boundary.
///
/// This is intentionally a token-shape discriminator, not a second parser.
/// Once claimed, Reference, NumericAtom, TargetList, and DamageEffect own the
/// actual recognition and any committed failure remains visible.
pub(in crate::oracle::canonical) fn claims_promoted_plain_damage_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(subject_end) = effect_subject_end(input) else {
        return false;
    };
    let body = &tokens[subject_end..];
    tokens
        .get(subject_end)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("deals"))
        && body.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("damage") && window[1].raw.eq_ignore_ascii_case("to")
        })
        && tokens.last().is_some_and(|token| token.raw == ".")
        && body.iter().filter(|token| token.raw == ".").count() == 1
        && !body.iter().any(|token| {
            matches!(token.raw.as_str(), "," | ";" | "•")
                || matches!(
                    token.raw.to_ascii_lowercase().as_str(),
                    "and" | "then" | "if" | "unless" | "instead"
                )
        })
}

pub(in crate::oracle::canonical) fn lower_single_target(
    targets: &TargetListNode,
) -> (&'static str, Value, i64, i64) {
    let slot = targets
        .slots
        .first()
        .expect("the damage automaton requires exactly one target slot");
    let (target_id, mut candidates) = lower_target_selector_candidates(&slot.selector)
        .expect("the damage automaton rejects unsupported target selectors");
    if slot.exclude_source {
        candidates["excludeSource"] = Value::Bool(true);
    }
    (
        target_id,
        candidates,
        i64::from(slot.cardinality.minimum),
        i64::from(slot.cardinality.maximum),
    )
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::oracle::canonical) struct LoweredTargetedDamageEffect {
    pub decision: Value,
    pub effect: Value,
}

pub(in crate::oracle::canonical) fn default_damage_target_id(
    effect: &DamageEffectNode,
) -> Option<&'static str> {
    let DamageRecipientNode::Targets(targets) = &effect.recipient else {
        return None;
    };
    Some(lower_single_target(targets).0)
}

/// Lowers one targeted damage atom and its target declaration together.
/// Triggered, spell, and activated envelopes can therefore share selector and
/// cardinality lowering without rebuilding target JSON locally.
pub(in crate::oracle::canonical) fn lower_targeted_damage_effect(
    effect: &DamageEffectNode,
    decision_id: &str,
) -> Option<LoweredTargetedDamageEffect> {
    let DamageRecipientNode::Targets(targets) = &effect.recipient else {
        return None;
    };
    let (_, candidates, minimum, maximum) = lower_single_target(targets);
    let source = match &effect.source {
        LocalObjectReference::SourceSelf => self_ref(),
    };
    Some(LoweredTargetedDamageEffect {
        decision: target_decision(decision_id, candidates, minimum, maximum),
        effect: json!({
            "kind": "dealDamage",
            "source": source,
            "amount": lower_numeric_expression(&effect.amount)?,
            "recipient": chosen_target(decision_id),
        }),
    })
}

enum DamageFamilySequenceLoweringError {
    ExistingUnsupported,
    SharedPermissionSupport(TargetlessLoweringError),
    SharedPlayerSetSupport(TargetlessLoweringError),
}

fn damage_sequence_lowering_failure(
    cursor: &mut OracleTokenCursor<'_>,
    error: DamageFamilySequenceLoweringError,
) -> MachineFailure {
    match error {
        DamageFamilySequenceLoweringError::ExistingUnsupported => {
            cursor.fail("an effect sequence supported by the damage-family lowering")
        }
        DamageFamilySequenceLoweringError::SharedPermissionSupport(error) => {
            cursor.enter("lower.permission.temporary.shared-support");
            cursor.fail(error.expectation())
        }
        DamageFamilySequenceLoweringError::SharedPlayerSetSupport(error) => {
            cursor.enter("lower.damage.player-set.shared-support");
            cursor.fail(error.expectation())
        }
    }
}

fn lower_local_effect_sequence(
    sequence: EffectSequenceNode,
    transitions: &[MachineTransition],
) -> Result<CanonicalRuleDraft, DamageFamilySequenceLoweringError> {
    let targeted_damage_count = sequence
        .effects
        .iter()
        .filter(|effect| {
            matches!(
                effect,
                LocalEffectNode::DealDamage(DamageEffectNode {
                    recipient: DamageRecipientNode::Targets(_),
                    ..
                })
            )
        })
        .count();
    let uses_x = sequence.effects.iter().any(|effect| match effect {
        LocalEffectNode::CardAction(_)
        | LocalEffectNode::OptionalSequence(_)
        | LocalEffectNode::Retarget(_)
        | LocalEffectNode::CreateExplicitTokens(_) => false,
        LocalEffectNode::CounterSpell(_) => false,
        LocalEffectNode::TemporaryKeywordGrant(_) => false,
        LocalEffectNode::AddMana(_) => false,
        LocalEffectNode::DealDamage(effect) => effect.amount.uses_x(),
        LocalEffectNode::GainLife(effect) => {
            effect.amount == NumericExpression::Atom(NumericAtom::VariableX)
        }
        LocalEffectNode::LoseLife(effect) => {
            effect.amount == NumericExpression::Atom(NumericAtom::VariableX)
        }
        LocalEffectNode::DrawCards(effect) => matches!(
            &effect.count,
            NumericExpression::Atom(NumericAtom::VariableX)
        ),
        LocalEffectNode::OptionalDrawCards(effect) => matches!(
            &effect.maximum,
            NumericExpression::Atom(NumericAtom::VariableX)
        ),
        LocalEffectNode::Amass(effect) => matches!(
            &effect.count,
            NumericExpression::Atom(NumericAtom::VariableX)
        ),
        LocalEffectNode::EmpowerJace(effect) => matches!(
            &effect.count,
            NumericExpression::Atom(NumericAtom::VariableX)
        ),
        LocalEffectNode::PermanentKeywordAction(PermanentKeywordActionNode {
            quantity, ..
        })
        | LocalEffectNode::ControllerKeywordAction(ControllerKeywordActionNode {
            quantity, ..
        }) => {
            matches!(
                quantity,
                Some(NumericExpression::Atom(NumericAtom::VariableX))
            )
        }
        LocalEffectNode::PutCounters(_)
        | LocalEffectNode::Investigate(_)
        | LocalEffectNode::DiscardCards(_)
        | LocalEffectNode::Scry(_)
        | LocalEffectNode::Surveil(_)
        | LocalEffectNode::Earthbend(_)
        | LocalEffectNode::TransformPermanent(_)
        | LocalEffectNode::MoveSource(_)
        | LocalEffectNode::MillCards(_)
        | LocalEffectNode::AnimateSource(_)
        | LocalEffectNode::MoveCards(_)
        | LocalEffectNode::TargetCardZoneChange(_)
        | LocalEffectNode::PlayerAction(_)
        | LocalEffectNode::TargetPlayerLife(_)
        | LocalEffectNode::ZoneBatch(_)
        | LocalEffectNode::ZoneChange(_)
        | LocalEffectNode::CreateTokens(_)
        | LocalEffectNode::CreateNamedTokens(_)
        | LocalEffectNode::OptionalLibrarySearch(_)
        | LocalEffectNode::ControllerLibrarySearch(_)
        | LocalEffectNode::OptionalCardDeployment(_)
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
        | LocalEffectNode::PreventDamage(_)
        | LocalEffectNode::Attach(_)
        | LocalEffectNode::Choose(_)
        | LocalEffectNode::UntapTarget(_) => false,
    });
    let mut decisions = uses_x.then(x_value).into_iter().collect::<Vec<_>>();
    let mut effects = Vec::with_capacity(sequence.effects.len());
    let mut damage_index = 0;

    for (effect_index, effect) in sequence.effects.into_iter().enumerate() {
        let shared_error = match lower_targetless_local_effects(
            &effect,
            TargetlessDeclarationSupport::XValue,
            Some(&format!("damageEffect{effect_index}")),
        ) {
            Ok(lowered) => {
                effects.extend(lowered);
                continue;
            }
            Err(error) => error,
        };
        match effect {
            LocalEffectNode::CardAction(_) | LocalEffectNode::OptionalSequence(_) => {
                return Err(DamageFamilySequenceLoweringError::ExistingUnsupported);
            }
            LocalEffectNode::Retarget(_)
            | LocalEffectNode::CreateExplicitTokens(_)
            | LocalEffectNode::SelectedPermanents(_)
            | LocalEffectNode::TapPermanents(_)
            | LocalEffectNode::PermanentKeywordAction(_)
            | LocalEffectNode::PreventDamage(_)
            | LocalEffectNode::Attach(_)
            | LocalEffectNode::Choose(_) => {
                return Err(DamageFamilySequenceLoweringError::ExistingUnsupported);
            }
            LocalEffectNode::TemporaryPlayerPermission(_) => {
                return Err(DamageFamilySequenceLoweringError::SharedPermissionSupport(
                    shared_error,
                ));
            }
            LocalEffectNode::OptionalDrawCards(_) => {
                return Err(DamageFamilySequenceLoweringError::ExistingUnsupported);
            }
            LocalEffectNode::CounterSpell(_) | LocalEffectNode::AddMana(_) => {
                unreachable!("damage-family lowering does not claim counter effects")
            }
            LocalEffectNode::DealDamage(effect) => {
                let source = match &effect.source {
                    LocalObjectReference::SourceSelf => self_ref(),
                };
                match &effect.recipient {
                    DamageRecipientNode::Targets(_) => {
                        let base_id = default_damage_target_id(&effect)
                            .expect("targeted damage has a default target identifier");
                        let target_id = if targeted_damage_count == 1 {
                            base_id.to_string()
                        } else {
                            format!("{base_id}{damage_index}")
                        };
                        damage_index += 1;
                        let lowered = lower_targeted_damage_effect(&effect, &target_id)
                            .ok_or(DamageFamilySequenceLoweringError::ExistingUnsupported)?;
                        decisions.push(lowered.decision);
                        effects.push(lowered.effect);
                    }
                    DamageRecipientNode::Player(_) => {
                        unreachable!(
                            "targetless damage is lowered by the shared targetless lowerer"
                        )
                    }
                    DamageRecipientNode::PlayerSet(_) => {
                        return Err(DamageFamilySequenceLoweringError::SharedPlayerSetSupport(
                            shared_error,
                        ));
                    }
                    DamageRecipientNode::EachPermanent(predicate) => {
                        effects.push(json!({
                            "kind": "dealDamage",
                            "source": source,
                            "amount": lower_numeric_expression(&effect.amount)
                                .ok_or(DamageFamilySequenceLoweringError::ExistingUnsupported)?,
                            "recipient": {
                                "kind": "eachPermanent",
                                "where": lower_permanent_object_predicate(predicate),
                            },
                        }));
                    }
                }
            }
            LocalEffectNode::GainLife(effect) => {
                let player = match effect.player {
                    LocalPlayerReference::Controller => controller(),
                    LocalPlayerReference::ControllerOfQualified(_) => {
                        return Err(DamageFamilySequenceLoweringError::ExistingUnsupported);
                    }
                    LocalPlayerReference::ControllerOf(_) => {
                        unreachable!("damage/life sequence has no target-controller binding")
                    }
                };
                effects.push(json!({
                    "kind": "gainLife",
                    "player": player,
                    "amount": lower_numeric_expression(&effect.amount)
                        .expect("damage/life sequence has no target-property binding"),
                }));
            }
            LocalEffectNode::LoseLife(effect) => {
                let player = match effect.player {
                    LocalPlayerReference::Controller => controller(),
                    LocalPlayerReference::ControllerOfQualified(_) => {
                        return Err(DamageFamilySequenceLoweringError::ExistingUnsupported);
                    }
                    LocalPlayerReference::ControllerOf(_) => {
                        unreachable!("damage/life sequence has no target-controller binding")
                    }
                };
                effects.push(json!({
                    "kind": "loseLife",
                    "player": player,
                    "amount": lower_numeric_expression(&effect.amount)
                        .expect("damage/life sequence has no target-property binding"),
                }));
            }
            LocalEffectNode::DrawCards(_)
            | LocalEffectNode::TemporaryKeywordGrant(_)
            | LocalEffectNode::MillCards(_)
            | LocalEffectNode::AnimateSource(_)
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
            | LocalEffectNode::MoveCards(_)
            | LocalEffectNode::TargetCardZoneChange(_)
            | LocalEffectNode::PlayerAction(_)
            | LocalEffectNode::TargetPlayerLife(_)
            | LocalEffectNode::ZoneBatch(_)
            | LocalEffectNode::MoveSource(_)
            | LocalEffectNode::ZoneChange(_)
            | LocalEffectNode::CreateTokens(_)
            | LocalEffectNode::CreateNamedTokens(_)
            | LocalEffectNode::OptionalLibrarySearch(_)
            | LocalEffectNode::ControllerLibrarySearch(_)
            | LocalEffectNode::OptionalCardDeployment(_)
            | LocalEffectNode::LoseAbilities(_)
            | LocalEffectNode::DestroyReferencedSet(_)
            | LocalEffectNode::DestroyCollection(_)
            | LocalEffectNode::Sacrifice(_)
            | LocalEffectNode::ModifyStats(_)
            | LocalEffectNode::BecomeTypes(_)
            | LocalEffectNode::QuantifiedPlayerAction(_)
            | LocalEffectNode::UntapTarget(_) => {
                unreachable!("damage-family lowering does not claim zone-change/search effects")
            }
        }
    }

    let trace = transitions
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": decisions,
            },
            "effects": effects,
        }),
        &[
            "Run the shared local effect automata over one token stream",
            "Resolve source, numeric expression, target selector, and connector submachines",
            "Lower typed effects to independent primitive IR operations",
            &trace,
        ],
    ))
}

/// Existing adapters retain their historical IDs and IR. A newly recognized
/// population node belongs to the contextual program parent, which validates
/// actual face declarations instead of manufacturing a producer from X use.
pub(in crate::oracle::canonical) enum DamageFamilyParseAttempt {
    Parsed(CanonicalRuleDraft),
    DeferredToContextualProgram,
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_promoted_damage_family(
    input: &AbilityInput<'_>,
    automaton: &'static str,
) -> DamageFamilyParseAttempt {
    let mut cursor = OracleTokenCursor::new(automaton, input.tokens);
    let checkpoint = cursor.checkpoint();
    let sequence = match recognize_effect_sequence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        Ok(sequence) => sequence,
        Err(failure) => return DamageFamilyParseAttempt::Failed(failure),
    };
    if sequence.effects.iter().any(|effect| {
        let subject = match effect {
            LocalEffectNode::PutCounters(node) => Some(&node.permanent),
            LocalEffectNode::TemporaryKeywordGrant(node) => Some(&node.objects),
            LocalEffectNode::ModifyStats(node) => Some(&node.targets),
            LocalEffectNode::BecomeTypes(node) => Some(&node.objects),
            _ => None,
        };
        if subject.is_some_and(|subject| {
            matches!(
                subject,
                PermanentOperandNode::Targets(_) | PermanentOperandNode::BoundReference(_)
            )
        }) {
            return true;
        }

        matches!(
            effect,
            LocalEffectNode::DealDamage(DamageEffectNode {
                recipient: DamageRecipientNode::PlayerSet(_),
                ..
            }) | LocalEffectNode::PlayerAction(_)
                | LocalEffectNode::SelectedPermanents(_)
                | LocalEffectNode::TapPermanents(_)
                | LocalEffectNode::PermanentKeywordAction(_)
                | LocalEffectNode::PreventDamage(_)
                | LocalEffectNode::Attach(_)
                | LocalEffectNode::Choose(_)
                | LocalEffectNode::DiscardCards(_)
                | LocalEffectNode::Scry(_)
                | LocalEffectNode::Retarget(_)
                | LocalEffectNode::CreateExplicitTokens(_)
        ) || matches!(
            effect,
            LocalEffectNode::Sacrifice(SacrificeEffectNode::Selected { .. })
        ) || matches!(effect, LocalEffectNode::CreateNamedTokens(creation)
            if matches!(creation.creation, NamedTokenCreationPattern::OneOf(_)))
    }) {
        cursor.rollback(checkpoint);
        return DamageFamilyParseAttempt::DeferredToContextualProgram;
    }
    match lower_local_effect_sequence(sequence, cursor.transitions()) {
        Ok(draft) => DamageFamilyParseAttempt::Parsed(draft),
        Err(error) => {
            DamageFamilyParseAttempt::Failed(damage_sequence_lowering_failure(&mut cursor, error))
        }
    }
}

fn lower_conditional_damage_effect(
    node: ConditionalEffectNode,
    transitions: &[MachineTransition],
) -> Option<CanonicalRuleDraft> {
    let antecedent = node
        .else_effects
        .first()
        .and_then(|effect| match effect {
            LocalEffectNode::CardAction(_) | LocalEffectNode::OptionalSequence(_) => None,
            LocalEffectNode::Retarget(_) | LocalEffectNode::CreateExplicitTokens(_) => None,
            LocalEffectNode::DealDamage(effect) => Some(effect),
            LocalEffectNode::CounterSpell(_)
            | LocalEffectNode::TemporaryKeywordGrant(_)
            | LocalEffectNode::AddMana(_)
            | LocalEffectNode::DrawCards(_)
            | LocalEffectNode::OptionalDrawCards(_)
            | LocalEffectNode::MillCards(_)
            | LocalEffectNode::AnimateSource(_)
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
            | LocalEffectNode::MoveCards(_)
            | LocalEffectNode::TargetCardZoneChange(_)
            | LocalEffectNode::PlayerAction(_)
            | LocalEffectNode::TargetPlayerLife(_)
            | LocalEffectNode::ZoneBatch(_)
            | LocalEffectNode::MoveSource(_)
            | LocalEffectNode::ZoneChange(_)
            | LocalEffectNode::CreateTokens(_)
            | LocalEffectNode::CreateNamedTokens(_)
            | LocalEffectNode::OptionalLibrarySearch(_)
            | LocalEffectNode::ControllerLibrarySearch(_)
            | LocalEffectNode::OptionalCardDeployment(_)
            | LocalEffectNode::GainLife(_)
            | LocalEffectNode::LoseLife(_)
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
            | LocalEffectNode::PreventDamage(_)
            | LocalEffectNode::Attach(_)
            | LocalEffectNode::Choose(_)
            | LocalEffectNode::UntapTarget(_) => None,
        })
        .expect("the conditional damage automaton always emits its antecedent branch");
    let DamageRecipientNode::Targets(antecedent_target) = &antecedent.recipient else {
        unreachable!("conditional damage replacement requires a targeted antecedent")
    };
    let (target_id, candidates, minimum, maximum) = lower_single_target(antecedent_target);
    let lower_effects = |effects: Vec<LocalEffectNode>| {
        effects
            .into_iter()
            .map(|effect| match effect {
                LocalEffectNode::CardAction(_) | LocalEffectNode::OptionalSequence(_) => None,
                LocalEffectNode::Retarget(_)
                | LocalEffectNode::CreateExplicitTokens(_)
                | LocalEffectNode::SelectedPermanents(_)
                | LocalEffectNode::TapPermanents(_)
                | LocalEffectNode::PermanentKeywordAction(_)
                | LocalEffectNode::PreventDamage(_)
                | LocalEffectNode::Attach(_)
                | LocalEffectNode::Choose(_) => None,
                LocalEffectNode::DealDamage(effect) => Some({
                    let DamageRecipientNode::Targets(_) = effect.recipient else {
                        unreachable!("conditional damage replacement keeps a targeted recipient")
                    };
                    json!({
                        "kind": "dealDamage",
                        "source": match effect.source {
                            LocalObjectReference::SourceSelf => self_ref(),
                        },
                        "amount": lower_numeric_expression(&effect.amount)?,
                        "recipient": chosen_target(target_id),
                    })
                }),
                LocalEffectNode::GainLife(effect) => Some(json!({
                    "kind": "gainLife",
                    "player": match effect.player {
                        LocalPlayerReference::Controller => controller(),
                        LocalPlayerReference::ControllerOfQualified(_) => return None,
                        LocalPlayerReference::ControllerOf(_) => {
                            unreachable!("conditional damage has no target-controller binding")
                        }
                    },
                    "amount": lower_numeric_expression(&effect.amount)
                        .expect("conditional damage has no target-property binding"),
                })),
                LocalEffectNode::LoseLife(effect) => Some(json!({
                    "kind": "loseLife",
                    "player": match effect.player {
                        LocalPlayerReference::Controller => controller(),
                        LocalPlayerReference::ControllerOfQualified(_) => return None,
                        LocalPlayerReference::ControllerOf(_) => {
                            unreachable!("conditional damage has no target-controller binding")
                        }
                    },
                    "amount": lower_numeric_expression(&effect.amount)
                        .expect("conditional damage has no target-property binding"),
                })),
                LocalEffectNode::OptionalDrawCards(_) => None,
                LocalEffectNode::CounterSpell(_)
                | LocalEffectNode::TemporaryKeywordGrant(_)
                | LocalEffectNode::AddMana(_)
                | LocalEffectNode::DrawCards(_)
                | LocalEffectNode::MillCards(_)
                | LocalEffectNode::AnimateSource(_)
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
                | LocalEffectNode::MoveCards(_)
                | LocalEffectNode::TargetCardZoneChange(_)
                | LocalEffectNode::PlayerAction(_)
                | LocalEffectNode::TargetPlayerLife(_)
                | LocalEffectNode::ZoneBatch(_)
                | LocalEffectNode::MoveSource(_)
                | LocalEffectNode::ZoneChange(_)
                | LocalEffectNode::CreateTokens(_)
                | LocalEffectNode::CreateNamedTokens(_)
                | LocalEffectNode::OptionalLibrarySearch(_)
                | LocalEffectNode::ControllerLibrarySearch(_)
                | LocalEffectNode::OptionalCardDeployment(_)
                | LocalEffectNode::LoseAbilities(_)
                | LocalEffectNode::DestroyReferencedSet(_)
                | LocalEffectNode::DestroyCollection(_)
                | LocalEffectNode::Sacrifice(_)
                | LocalEffectNode::ModifyStats(_)
                | LocalEffectNode::BecomeTypes(_)
                | LocalEffectNode::QuantifiedPlayerAction(_)
                | LocalEffectNode::TemporaryPlayerPermission(_)
                | LocalEffectNode::UntapTarget(_) => {
                    unreachable!(
                        "conditional damage lowering does not claim zone-change/search effects"
                    )
                }
            })
            .collect::<Option<Vec<_>>>()
    };
    let then_effects = lower_effects(node.then_effects)?;
    let else_effects = lower_effects(node.else_effects)?;
    let condition = lower_local_condition(&node.condition);
    let trace = transitions
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(target_id, candidates, minimum, maximum)],
            },
            "effects": [{
                "kind": "conditionalEffect",
                "condition": condition,
                "then": then_effects,
                "else": else_effects,
            }],
        }),
        &[
            "Recognize an antecedent effect and a later conditional replacement",
            "Bind the replacement source and target to their antecedents",
            "Lower both branches to the same primitive operation",
            &trace,
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_conditional_damage(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("conditional-effect-sequence", input.tokens);
    let node = recognize_conditional_replacement_sequence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    lower_conditional_damage_effect(node, cursor.transitions())
        .ok_or_else(|| cursor.fail("effects supported by conditional damage lowering"))
}

#[cfg(test)]
#[path = "damage_lowering_tests.rs"]
mod lowering_tests;

#[cfg(test)]
#[path = "player_damage_lowering_tests.rs"]
mod player_damage_lowering_tests;

#[cfg(test)]
#[path = "player_damage_context_deferral_tests.rs"]
mod player_damage_context_deferral_tests;
