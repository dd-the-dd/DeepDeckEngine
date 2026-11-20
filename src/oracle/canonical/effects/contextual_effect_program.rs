use super::super::*;
use crate::oracle::characteristic_vocabulary::{AcceptedForms, CardType, lookup_card_type};
use crate::oracle::syntax::{
    AbilityInput, classify_promoted_ability_from_tokens, root_activation_effect_start,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EffectProgramContext {
    Spell,
    Activated,
    Mana,
}

pub(in crate::oracle::canonical) enum EffectProgramParseAttempt {
    Parsed(CanonicalRuleDraft),
    Unpromoted(MachineFailure),
    Failed(MachineFailure),
    FailedSupport(MachineFailure),
    NotApplicable,
}

pub(in crate::oracle::canonical) fn spell_face(type_line: &str) -> bool {
    type_line
        .split('\u{2014}')
        .next()
        .unwrap_or(type_line)
        .split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty())
        .any(|word| {
            matches!(
                lookup_card_type(word, AcceptedForms::SingularOnly),
                Some(CardType::Instant | CardType::Sorcery)
            )
        })
}

/// A root replacement event is a different envelope from an imperative
/// program. `if` alone is not a discriminator: ordinary conditional effects
/// remain available to their shared child. Quotes and reminders own their text.
fn root_replacement_event(input: &AbilityInput<'_>) -> bool {
    if !input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"))
    {
        return false;
    }
    let mut quoted = false;
    let mut parentheses = 0_usize;
    let mut before_separator = true;
    let mut would = false;
    let mut instead = false;
    for token in input.tokens {
        match token.raw.as_str() {
            "\"" => quoted = !quoted,
            "(" if !quoted => parentheses += 1,
            ")" if !quoted => parentheses = parentheses.saturating_sub(1),
            _ if !quoted && parentheses == 0 => {
                if token.raw == "." {
                    break;
                }
                if token.raw == "," {
                    before_separator = false;
                }
                would |= before_separator && token.raw.eq_ignore_ascii_case("would");
                instead |= token.raw.eq_ignore_ascii_case("instead");
            }
            _ => {}
        }
    }
    would && instead
}

fn admit_context(input: &AbilityInput<'_>) -> Option<EffectProgramContext> {
    if claims_promoted_loyalty_ability_family(input)
        || matches!(
            classify_promoted_ability_from_tokens(input),
            Some("triggeredAbility" | "replacementEffect")
        )
        || root_replacement_event(input)
    {
        return None;
    }
    if root_activation_effect_start(input).is_some() {
        Some(EffectProgramContext::Activated)
    } else if spell_face(input.face_type_line) {
        Some(EffectProgramContext::Spell)
    } else {
        None
    }
}

fn effects_move_source_from_graveyard(sequence: &EffectSequenceNode) -> bool {
    sequence.effects.iter().any(|effect| match effect {
        LocalEffectNode::MoveSource(movement) => movement.from == SourceZone::Graveyard,
        LocalEffectNode::OptionalSequence(body) => effects_move_source_from_graveyard(body),
        _ => false,
    })
}

fn activation_zone_from_costs(
    cursor: &mut OracleTokenCursor<'_>,
    costs: &ActivationCostSequenceNode,
    effects: &EffectSequenceNode,
) -> MachineResult<Option<&'static str>> {
    let requirements = costs.source_zone_requirements();
    let needs_hand = requirements.hand;
    let needs_battlefield = requirements.battlefield;
    // An effect that moves the source out of the graveyard can only resolve
    // for an ability activated there, exactly like an exile-self cost.
    let needs_graveyard = requirements.graveyard || effects_move_source_from_graveyard(effects);
    cursor.enter("program.activation.source-zone-conflict");
    cursor.require(
        !(needs_hand && needs_battlefield) && !(needs_graveyard && (needs_hand || needs_battlefield)),
        "one coherent source zone: discard-self costs require hand, exile-self costs require graveyard, while tap-self and sacrifice-self costs require battlefield",
    )?;
    Ok(if needs_hand {
        Some("hand")
    } else if needs_graveyard {
        Some("graveyard")
    } else {
        None
    })
}

/// Context chooses when this program executes. The shared children choose
/// which actions it contains: no first-verb allowlist or combined-effect leaf.
/// Costs and body are each consumed once, then one shared lowering pass emits
/// independent primitive actions and declaration bindings.
pub(in crate::oracle::canonical) fn try_parse_contextual_effect_program(
    input: &AbilityInput<'_>,
) -> EffectProgramParseAttempt {
    let Some(context) = admit_context(input) else {
        return EffectProgramParseAttempt::NotApplicable;
    };
    let mut cursor = OracleTokenCursor::new("contextual-effect-program", input.tokens);
    // Activation X belongs to this activation. Never borrow the source's
    // retained casting X from a trigger-only recognition context.
    let recognition_context = EffectRecognitionContext::default()
        .with_zone_change_domains(ZoneChangeDomains::SpellOrBattlefield)
        .with_sentence_conditionals()
        .with_unless_frames();
    let mut restrictions = ActivationRestrictionsNode::default();
    let mut reduction = None;
    let (costs, effects, annotation) = match context {
        EffectProgramContext::Activated => {
            match try_recognize_activated_effect_sequence_with_cost_context(
                &mut cursor,
                input.name_links,
                input.name_resolution_failure,
                &recognition_context,
                |costs, base| activation_body_context(input.face_type_line, costs, base),
            ) {
                ActivatedEffectSequenceParseAttempt::Parsed(program) => {
                    restrictions = program.restrictions;
                    reduction = program.reduction;
                    (
                        Some(program.costs),
                        EffectProgramNode::Sequence(program.effects),
                        program.timing_annotation,
                    )
                }
                ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(failure) => {
                    return EffectProgramParseAttempt::Unpromoted(failure);
                }
                ActivatedEffectSequenceParseAttempt::FailedCommitted(failure) => {
                    return EffectProgramParseAttempt::Failed(failure);
                }
            }
        }
        EffectProgramContext::Spell => {
            match recognize_imperative_program_body(&mut cursor, input, &recognition_context) {
                Ok(program) => (None, program, None),
                Err(attempt) => return attempt,
            }
        }
        EffectProgramContext::Mana => unreachable!("mana context is derived from typed effects"),
    };
    let outcome = lower_contextual_effect_program_with_annotation(
        &mut cursor,
        context,
        costs,
        effects,
        annotation,
    );
    let outcome = apply_activation_cost_reduction(&mut cursor, outcome, reduction);
    apply_activation_restrictions(outcome, restrictions)
}

/// A generic activation-cost reduction is metadata of the activated ability
/// (CR 601.2f via 602.2b); a mana ability keeps no reduction slot.
fn apply_activation_cost_reduction(
    cursor: &mut OracleTokenCursor<'_>,
    outcome: EffectProgramParseAttempt,
    reduction: Option<ActivationCostReductionNode>,
) -> EffectProgramParseAttempt {
    let Some(reduction) = reduction else {
        return outcome;
    };
    let EffectProgramParseAttempt::Parsed(mut draft) = outcome else {
        return outcome;
    };
    cursor.enter("activation.reduction.owner");
    if draft.rule["kind"] != "activatedAbility" {
        return EffectProgramParseAttempt::FailedSupport(
            cursor.fail("an activated ability (not a mana ability) owning the cost reduction"),
        );
    }
    draft.rule["manaCostReduction"] = lower_activation_cost_reduction(&reduction);
    draft
        .operations
        .push("Attach the per-permanent activation cost reduction".to_string());
    EffectProgramParseAttempt::Parsed(draft)
}

/// The costless imperative body shared by spell abilities and other parents
/// whose resolution is an ordinary instruction program (CR 608.2). A program
/// operator either opens the body or opens a later sentence; otherwise the
/// shared sequence keeps its diagnostic.
fn recognize_imperative_program_body(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    recognition_context: &EffectRecognitionContext,
) -> Result<EffectProgramNode, EffectProgramParseAttempt> {
    let checkpoint = cursor.checkpoint();
    let optional_head = cursor.peek().is_some_and(|s| s.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|s| s.eq_ignore_ascii_case("may"))
        || later_optional_outcome_block(cursor).is_some();
    match try_recognize_effect_sequence_with_context(
        cursor,
        input.name_links,
        input.name_resolution_failure,
        recognition_context,
    ) {
        EffectSequenceParseAttempt::Parsed(effects) => Ok(EffectProgramNode::Sequence(effects)),
        EffectSequenceParseAttempt::RejectedCandidate(failure) if !optional_head => {
            Err(EffectProgramParseAttempt::Unpromoted(failure))
        }
        EffectSequenceParseAttempt::FailedCommitted(failure) if !optional_head => {
            Err(EffectProgramParseAttempt::Failed(failure))
        }
        EffectSequenceParseAttempt::RejectedCandidate(_)
        | EffectSequenceParseAttempt::FailedCommitted(_) => {
            cursor.rollback(checkpoint);
            recognize_effect_program_with_context(
                cursor,
                input.name_links,
                input.name_resolution_failure,
                recognition_context,
            )
            .map_err(EffectProgramParseAttempt::Failed)
        }
    }
}

/// A costless instruction program owned by a parent frame other than a spell
/// face (for example a Dungeon room, CR 309.4c). The parent bounds the cursor;
/// recognition and lowering are exactly those of a spell ability's body.
pub(in crate::oracle::canonical) fn recognize_and_lower_imperative_program(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
) -> MachineResult<(Value, Option<Value>)> {
    let recognition_context = EffectRecognitionContext::default()
        .with_zone_change_domains(ZoneChangeDomains::SpellOrBattlefield)
        .with_sentence_conditionals()
        .with_unless_frames();
    let program = recognize_imperative_program_body(cursor, input, &recognition_context).map_err(
        |attempt| match attempt {
            EffectProgramParseAttempt::Unpromoted(failure)
            | EffectProgramParseAttempt::Failed(failure)
            | EffectProgramParseAttempt::FailedSupport(failure) => failure,
            EffectProgramParseAttempt::Parsed(_) | EffectProgramParseAttempt::NotApplicable => {
                cursor.fail("an imperative instruction program")
            }
        },
    )?;
    match lower_contextual_effect_program_with_annotation(
        cursor,
        EffectProgramContext::Spell,
        None,
        program,
        None,
    ) {
        EffectProgramParseAttempt::Parsed(mut draft) => {
            let effects = draft.rule["effects"].take();
            let declaration = draft
                .rule
                .as_object_mut()
                .and_then(|rule| rule.remove("declaration"));
            Ok((effects, declaration))
        }
        EffectProgramParseAttempt::Unpromoted(failure)
        | EffectProgramParseAttempt::Failed(failure)
        | EffectProgramParseAttempt::FailedSupport(failure) => Err(failure),
        EffectProgramParseAttempt::NotApplicable => {
            Err(cursor.fail("an applicable imperative instruction program"))
        }
    }
}

/// CR 606: a loyalty ability is an activated ability whose cost is the signed
/// loyalty change. The body is the shared activated effect sequence; the
/// loyalty cost, sorcery timing and once-per-turn limit belong to the envelope
/// (CR 606.3), and a loyalty ability is never a mana ability (CR 605.1a).
pub(in crate::oracle::canonical) fn parse_loyalty_effect_program(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("loyalty-ability", input.tokens);
    cursor.enter("loyalty-ability.cost");
    let cost = recognize_loyalty_cost(&mut cursor)?;
    cursor.enter("loyalty-ability.separator");
    cursor.consume(":")?;
    cursor.enter("loyalty-ability.effects");
    let base = EffectRecognitionContext::default()
        .with_zone_change_domains(ZoneChangeDomains::SpellOrBattlefield)
        .with_sentence_conditionals()
        .with_unless_frames();
    let no_costs = ActivationCostSequenceNode {
        costs: Vec::new(),
        connectors: Vec::new(),
    };
    let mut context = activation_body_context(input.face_type_line, &no_costs, &base);
    context.ability_restriction_owner = AbilityRestrictionOwner::Activated;
    let effects = match try_recognize_effect_sequence_before_activation_annotation(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &context,
    ) {
        EffectSequenceParseAttempt::Parsed(effects) => effects,
        EffectSequenceParseAttempt::RejectedCandidate(failure)
        | EffectSequenceParseAttempt::FailedCommitted(failure) => return Err(failure),
    };
    cursor.enter("loyalty-ability.complete");
    cursor.finish()?;
    let mut draft = match lower_contextual_effect_program(
        &mut cursor,
        EffectProgramContext::Activated,
        Some(no_costs),
        effects,
    ) {
        EffectProgramParseAttempt::Parsed(draft) => draft,
        EffectProgramParseAttempt::Unpromoted(failure)
        | EffectProgramParseAttempt::FailedSupport(failure)
        | EffectProgramParseAttempt::Failed(failure) => return Err(failure),
        EffectProgramParseAttempt::NotApplicable => {
            return Err(cursor.fail("an applicable activated effect program"));
        }
    };
    // The shared lowerer may classify an Add-only body with no costs as a
    // mana ability; loyalty abilities are excluded by CR 605.1a.
    draft.rule["kind"] = json!("activatedAbility");
    if draft.rule.get("declaration").is_none() {
        draft.rule["declaration"] = json!({ "kind": "castingDeclaration", "decisions": [] });
    }
    draft.rule["costs"] = json!([{
        "kind": "payLoyalty",
        "object": self_ref(),
        "amount": integer(cost.delta),
    }]);
    draft.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
    draft.rule["activationLimit"] = json!({ "kind": "oncePerTurn", "id": "loyaltyAbility" });
    draft
        .operations
        .push("Attach the loyalty cost, sorcery timing and once-per-turn limit (CR 606.3)".into());
    Ok(draft)
}

/// Restrictions belong to the activated ability itself, so they are applied
/// to the lowered rule rather than to any effect (CR 602.5b, 602.5d).
fn apply_activation_restrictions(
    outcome: EffectProgramParseAttempt,
    restrictions: ActivationRestrictionsNode,
) -> EffectProgramParseAttempt {
    let EffectProgramParseAttempt::Parsed(mut draft) = outcome else {
        return outcome;
    };
    if restrictions.is_empty() {
        return EffectProgramParseAttempt::Parsed(draft);
    }
    // Sorcery timing already implies the controller's own turn (CR 307.1).
    let timing = if restrictions.sorcery_timing {
        Some(json!({ "kind": "sorceryTiming" }))
    } else if restrictions.controller_turn {
        Some(json!({ "kind": "duringControllerTurn", "player": controller() }))
    } else {
        None
    };
    let condition = restrictions.condition.as_ref().map(lower_static_condition);
    match (timing, condition) {
        (Some(timing), Some(condition)) => {
            draft.rule["activationCondition"] = and(vec![timing, condition]);
        }
        (Some(only), None) | (None, Some(only)) => draft.rule["activationCondition"] = only,
        (None, None) => {}
    }
    if restrictions.once_each_turn {
        draft.rule["activationLimit"] =
            json!({ "kind": "oncePerTurn", "id": "activationRestriction" });
    }
    draft
        .operations
        .push("Apply `Activate only` restrictions to the activated ability".to_string());
    EffectProgramParseAttempt::Parsed(draft)
}

/// Default battlefield activation plus retained-source costs proves only the
/// explicit source subject domain. Owner identity is not an anaphor binding.
fn activation_body_context(
    face_type_line: &str,
    costs: &ActivationCostSequenceNode,
    base: &EffectRecognitionContext,
) -> EffectRecognitionContext {
    use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
    let retained = !costs.source_zone_requirements().hand
        && !costs.source_zone_requirements().graveyard
        && costs.costs.iter().all(|cost| match cost {
            LocalActivationCostNode::PayMana(_)
            | LocalActivationCostNode::TapSelf(_)
            | LocalActivationCostNode::PayLife(_)
            | LocalActivationCostNode::TapObjects(_)
            | LocalActivationCostNode::DiscardCard(_)
            | LocalActivationCostNode::RemoveSourceCounters(_)
            | LocalActivationCostNode::RemoveControlledCounters(_)
            | LocalActivationCostNode::ExileCards(_) => true,
            LocalActivationCostNode::SacrificeSelf(_)
            | LocalActivationCostNode::SacrificeObject(_)
            | LocalActivationCostNode::DiscardSelf(_)
            | LocalActivationCostNode::ExileSelf(_) => false,
        });
    let mut context = base
        .clone()
        .with_source_attachment(source_attachment_role(face_type_line));
    context.source = None;
    context.source_cast_x = false;
    // A card discarded, exiled or sacrificed to pay for its own ability is
    // the only object `it` can name: the ability's source, read through its
    // last known information once it has left its zone (CR 113.7a, 602.2).
    if costs.costs.iter().any(|cost| {
        matches!(
            cost,
            LocalActivationCostNode::DiscardSelf(_)
                | LocalActivationCostNode::ExileSelf(_)
                | LocalActivationCostNode::SacrificeSelf(_)
        )
    }) {
        context = context.with_source_event_subject(LocalObjectReference::SourceSelf);
    }
    // `Remove a counter from this creature: It deals 1 damage ...`: a cost
    // that names the source while it stays on the battlefield is the only
    // object antecedent when no other cost names an object.
    let names_other_object = costs.costs.iter().any(|cost| {
        matches!(
            cost,
            LocalActivationCostNode::TapObjects(_)
                | LocalActivationCostNode::SacrificeObject(_)
                | LocalActivationCostNode::ExileCards(_)
                | LocalActivationCostNode::DiscardCard(_)
                | LocalActivationCostNode::RemoveControlledCounters(_)
        )
    });
    if !names_other_object
        && costs
            .costs
            .iter()
            .any(|cost| matches!(cost, LocalActivationCostNode::RemoveSourceCounters(_)))
    {
        context = context.with_source_event_subject(LocalObjectReference::SourceSelf);
    }
    context.source_numeric_reads =
        type_line_has_permanent_card_type(face_type_line) && !spell_face(face_type_line);
    context.activation_source_availability = if retained
        && type_line_has_permanent_card_type(face_type_line)
        && !spell_face(face_type_line)
    {
        ExplicitActivationSourceAvailability::RetainedBattlefieldPermanent
    } else {
        ExplicitActivationSourceAvailability::Unproved
    };
    context
}

/// Lower already recognized nodes with one shared binding/context policy.
/// Both the general program parent and compact Add-led envelope call this
/// adapter without reading costs or effects again.
pub(in crate::oracle::canonical) fn lower_contextual_effect_program(
    cursor: &mut OracleTokenCursor<'_>,
    context: EffectProgramContext,
    costs: Option<ActivationCostSequenceNode>,
    effects: EffectSequenceNode,
) -> EffectProgramParseAttempt {
    lower_contextual_effect_program_with_annotation(
        cursor,
        context,
        costs,
        EffectProgramNode::Sequence(effects),
        None,
    )
}

fn lower_contextual_effect_program_with_annotation(
    cursor: &mut OracleTokenCursor<'_>,
    context: EffectProgramContext,
    costs: Option<ActivationCostSequenceNode>,
    program: EffectProgramNode,
    annotation: Option<ActivationTimingAnnotation>,
) -> EffectProgramParseAttempt {
    let body = AbilityBodyNode {
        context,
        costs,
        program,
        annotation,
    };
    match resolve_ability_body_scopes(body) {
        Ok(AbilityBodyResolution::ScopeChecked(checked)) => {
            lower_scope_checked_ability_body(cursor, checked)
        }
        // Existing native domains not migrated to this resolution slice retain
        // their existing lowering checks. A resolution error never takes this path.
        Ok(AbilityBodyResolution::OutsideSlice(body)) => lower_native_ability_body(cursor, body),
        Err(failure) => {
            cursor.enter(failure.state);
            if failure.support_failure {
                EffectProgramParseAttempt::FailedSupport(cursor.fail(failure.expectation))
            } else {
                EffectProgramParseAttempt::Failed(cursor.fail(failure.expectation))
            }
        }
    }
}

fn lower_scope_checked_ability_body(
    cursor: &mut OracleTokenCursor<'_>,
    checked: ScopeCheckedAbilityBody,
) -> EffectProgramParseAttempt {
    lower_native_ability_body(cursor, checked.into_body())
}

fn lower_native_ability_body(
    cursor: &mut OracleTokenCursor<'_>,
    body: AbilityBodyNode,
) -> EffectProgramParseAttempt {
    let AbilityBodyNode {
        mut context,
        costs,
        program,
        annotation,
    } = body;
    let effects = program.inner_sequence();
    let activation_zone = if let Some(costs) = costs.as_ref() {
        match activation_zone_from_costs(cursor, costs, effects) {
            Ok(zone) => zone,
            Err(failure) => return EffectProgramParseAttempt::FailedSupport(failure),
        }
    } else {
        None
    };
    let mut steps = Vec::new();
    for step in program.steps() {
        let lowered = lower_program_step(
            cursor,
            step,
            &mut |cursor, sequence| match try_lower_local_effect_sequence_with_context(
                cursor,
                sequence,
                EffectSequenceLoweringContext::ValidateXAfterCardComposition,
            ) {
                EffectSequenceLoweringAttempt::Lowered(lowered) => Ok(lowered),
                EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure) => {
                    Err(EffectProgramParseAttempt::Unpromoted(failure))
                }
                EffectSequenceLoweringAttempt::FailedSupport(failure) => {
                    Err(EffectProgramParseAttempt::FailedSupport(failure))
                }
                EffectSequenceLoweringAttempt::FailedInvariant(failure) => {
                    Err(EffectProgramParseAttempt::Failed(failure))
                }
            },
            EffectProgramParseAttempt::FailedSupport,
        );
        match lowered {
            Ok(lowered) => steps.push(lowered),
            Err(attempt) => return attempt,
        }
    }
    let lowered = match concatenate_program_steps(cursor, steps) {
        Ok(lowered) => lowered,
        Err(failure) => return EffectProgramParseAttempt::FailedSupport(failure),
    };
    let mut decisions = costs
        .as_ref()
        .map(lower_activation_cost_decisions)
        .unwrap_or_default();
    decisions.extend(lowered.decisions);
    if context == EffectProgramContext::Activated {
        let Some(costs) = costs.as_ref() else {
            unreachable!("activated programs retain their parsed activation costs");
        };
        let facts = ManaProgramFacts::from_effect_sequence(&effects);
        let requires_target = decisions
            .iter()
            .any(|decision| decision["kind"].as_str() == Some("chooseTargets"));
        if annotation == Some(ActivationTimingAnnotation::InstantTimingExplanation)
            && !requires_target
        {
            cursor.enter("activation.annotation.nonmana-support");
            return EffectProgramParseAttempt::FailedSupport(cursor.fail(
                "a declared target proving stack-based activation timing for this explanation",
            ));
        }
        if facts.qualifies_as_mana_ability(costs, requires_target) {
            if let Err(failure) = require_supported_mana_program(cursor, costs, &facts) {
                return EffectProgramParseAttempt::FailedSupport(failure);
            }
            context = EffectProgramContext::Mana;
        }
    }
    let mut rule = json!({
        "kind": match context {
            EffectProgramContext::Spell => "spellAbility",
            EffectProgramContext::Activated => "activatedAbility",
            EffectProgramContext::Mana => "manaAbility",
        },
        "source": self_ref(),
        "effects": lowered.effects,
    });
    if let Some(costs) = costs {
        rule["costs"] = json!(
            costs
                .costs
                .iter()
                .flat_map(lower_local_activation_costs)
                .collect::<Vec<_>>()
        );
    }
    if let Some(zone) = activation_zone {
        rule["activationZone"] = json!(zone);
    }
    // Preserve the established activated-program declaration shape while
    // targetless spell and mana rules omit an unnecessary empty declaration.
    if context == EffectProgramContext::Activated || !decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        });
    }
    EffectProgramParseAttempt::Parsed(draft(
        rule,
        &[
            "Admit a program from its source-face and root ability envelope",
            "Consume typed costs and the shared effect sequence once",
            "Lower every action and binding through the shared primitive adapter",
            "Derive mana execution context from immediate typed actions and target declarations",
        ],
    ))
}

/// The compact envelope has already consumed one AddMana action and
/// its sentence-separated followups. Preserve that order in the ordinary
/// shared program AST; CR605.1a decides its context in the same lowerer.
pub(in crate::oracle::canonical) fn lower_recognized_add_led_activation(
    cursor: &mut OracleTokenCursor<'_>,
    ability: ActivatedManaAbilityNode,
) -> EffectProgramParseAttempt {
    let mut effects = vec![LocalEffectNode::AddMana(ability.effect)];
    let mut connectors = Vec::new();
    let mut where_x = Vec::new();
    let mut conditionals = Vec::new();
    let mut unless = Vec::new();
    if let Some(followup) = ability.followup {
        cursor.enter("activation.mana.program.followup");
        if let Err(failure) = cursor.require(
            !followup.effects.is_empty() && followup.connectors.len() + 1 == followup.effects.len(),
            "a nonempty followup with one connector between each action",
        ) {
            return EffectProgramParseAttempt::Failed(failure);
        }
        connectors.push(EffectConnector::Then);
        connectors.extend(followup.connectors);
        // The followup actions follow the leading AddMana action.
        where_x.extend(
            followup
                .where_x
                .into_iter()
                .map(|binding| WhereXBindingNode {
                    first_effect: binding.first_effect + 1,
                    end_effect: binding.end_effect + 1,
                    expression: binding.expression,
                }),
        );
        conditionals.extend(
            followup
                .conditionals
                .iter()
                .map(|conditional| conditional.shifted(1)),
        );
        unless.extend(followup.unless.into_iter().map(|frame| frame.shifted(1)));
        effects.extend(followup.effects);
    }
    lower_contextual_effect_program(
        cursor,
        EffectProgramContext::Activated,
        Some(ability.costs),
        EffectSequenceNode {
            effects,
            connectors,
            where_x,
            conditionals,
            unless,
        },
    )
}

#[cfg(test)]
#[path = "contextual_effect_program_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "activation_source_context_tests.rs"]
mod activation_source_tests;

#[cfg(test)]
#[path = "restored_shared_actions_tests.rs"]
mod restored_shared_actions_tests;
