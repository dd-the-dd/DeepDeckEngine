use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

/// Claims one atomic targeted battlefield zone change, optionally followed by
/// a parenthetical batch-definition reminder. Child automata own the selector
/// and reminder content after this token-shape boundary commits.
pub(in crate::oracle::canonical) fn claims_promoted_atomic_zone_change_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let action = tokens.first().is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("destroy")
            || is_verb_form(&token.raw, VerbLemma::Exile, VerbForm::Base)
            || is_verb_form(&token.raw, VerbLemma::Return, VerbForm::Base)
    });
    let targeted = tokens.get(1).is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("target") || token.raw.eq_ignore_ascii_case("up")
    }) && !starts_possessive_target_player(&OracleTokenCursor::new(
        "atomic-zone-change-head",
        &tokens[1..],
    ));
    let Some(first_terminator) = tokens.iter().position(|token| token.raw == ".") else {
        return false;
    };
    let suffix = &tokens[first_terminator + 1..];
    action
        && targeted
        && (suffix.is_empty() || suffix.first().is_some_and(|token| token.raw == "("))
}

pub(in crate::oracle::canonical) fn parse_promoted_atomic_zone_change(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("atomic-targeted-zone-change", input.tokens);
    let effect = recognize_zone_change_effect_with_policy(
        &mut cursor,
        ZoneChangeTargetSupport::TargetCollection,
        ZoneChangeDomains::SpellOrBattlefield,
    )?;
    let LocalEffectNode::ZoneChange(effect) = effect else {
        return Err(cursor.fail("an atomic targeted zone change"));
    };
    cursor.enter("effect.zone-change.terminator");
    cursor.consume(".")?;

    if cursor.peek().is_some_and(|raw| raw == "(") {
        let reminder = recognize_characteristic_batch_reminder(&mut cursor)?;
        cursor.enter("effect.zone-change.reminder.batch-match");
        cursor.require(
            target_contains_batch(&effect.target, reminder.batch),
            "a reminder describing the target selector's characteristic batch",
        )?;
    }
    cursor.enter("effect.zone-change.full-consumption");
    cursor.finish()?;

    let mut used_base_ids = Vec::new();
    let (decisions, target) = lower_zone_change_targets(&mut cursor, &effect, |base, total| {
        let ordinal = used_base_ids.iter().filter(|used| **used == base).count();
        used_base_ids.push(base);
        super::effect_sequence::target_id_for_ordinal(base, ordinal, total)
    })?;
    let operation = lower_targeted_zone_change_node(&mut cursor, &effect, target)?;
    let trace = cursor
        .transitions()
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
            "effects": [operation],
        }),
        &[
            "Recognize one atomic targeted battlefield zone change",
            "Resolve subtype and characteristic-batch selector predicates",
            "Validate optional batch reminder from the shared registry",
            "Lower the batch into primitive subtype predicates",
            &trace,
        ],
    ))
}

/// Lower one already-parsed target slot without discarding its cardinality.
/// Existing scalar references remain byte-for-byte identical in emitted IR.
pub(in crate::oracle::canonical) fn lower_zone_change_target_operand(
    cursor: &mut OracleTokenCursor<'_>,
    effect: &ZoneChangeEffectNode,
    target_id: &str,
    support: ZoneChangeTargetSupport,
) -> MachineResult<Value> {
    cursor.enter("lower.zone-change.target-operand");
    cursor.require(
        effect.target.slots.len() == 1,
        "one zone-change target slot",
    )?;
    let slot = &effect.target.slots[0];
    if slot.cardinality.maximum == 1 {
        return Ok(chosen_target(target_id));
    }
    cursor.require(
        slot.cardinality.maximum > 1 && support == ZoneChangeTargetSupport::TargetCollection,
        "an explicitly supported plural zone-change operand",
    )?;
    Ok(json!({"kind":"chosenTargets", "id":target_id}))
}

/// Declarations for every written target slot, in order, and the one operand
/// the zone change acts on. `target_id(base, total)` names each declaration;
/// `total` counts the slots of this list sharing that base identifier. A sole
/// slot keeps its historical scalar or collection operand. Several slots form
/// one simultaneous collection (CR 115.3, 608.2b): each slot remains its own
/// declaration and an illegal member does not stop the others.
pub(in crate::oracle::canonical) fn lower_zone_change_targets(
    cursor: &mut OracleTokenCursor<'_>,
    effect: &ZoneChangeEffectNode,
    mut target_id: impl FnMut(&'static str, usize) -> String,
) -> MachineResult<(Vec<Value>, Value)> {
    cursor.enter("lower.zone-change.target-candidates");
    let slots = effect
        .target
        .slots
        .iter()
        .map(|slot| lower_zone_change_slot_candidates(effect, slot))
        .collect::<Option<Vec<_>>>();
    let slots = cursor.require_value(slots, "a supported zone-change target candidate policy")?;
    let mut decisions = Vec::new();
    let mut members = Vec::new();
    for ((base, candidates, minimum, maximum), slot) in slots.iter().zip(&effect.target.slots) {
        let total = slots.iter().filter(|(other, ..)| other == base).count();
        let id = target_id(base, total);
        let mut decision = target_decision(&id, candidates.clone(), *minimum, *maximum);
        // `X target`, `any number of target` (CR 601.2c): the slot's typed
        // count replaces the literal envelope; an unrepresented bound fails.
        if let Some(count) = &slot.count {
            cursor.enter("lower.zone-change.target-count");
            let applied = apply_target_count_to_decision(&mut decision, count);
            cursor.require_value(applied, "a declared target count the IR represents")?;
        }
        decisions.push(decision);
        members.push(json!({"kind":"chosenTargets", "id":id}));
    }
    if let [decision] = decisions.as_slice() {
        let id = decision["id"].as_str().unwrap_or_default().to_string();
        let operand = lower_zone_change_target_operand(
            cursor,
            effect,
            &id,
            ZoneChangeTargetSupport::TargetCollection,
        )?;
        return Ok((decisions, operand));
    }
    Ok((decisions, json!({"kind":"union", "sets": members})))
}

pub(in crate::oracle::canonical) fn lower_targeted_zone_change_node(
    cursor: &mut OracleTokenCursor<'_>,
    effect: &ZoneChangeEffectNode,
    target: Value,
) -> MachineResult<Value> {
    cursor.enter("lower.zone-change.regeneration-policy");
    cursor.require(
        effect.action == ZoneChangeAction::Destroy
            || effect.regeneration == ZoneChangeRegenerationPolicy::Allowed,
        "a regeneration prohibition only on a destroy action",
    )?;
    let mut operation = lower_targeted_zone_change_effect(effect.action, target);
    if let Some(until) = effect.until {
        cursor.enter("lower.zone-change.until");
        cursor.require(
            effect.action == ZoneChangeAction::Exile,
            "an `until` duration only on an exile action",
        )?;
        operation["until"] = match until {
            ZoneChangeUntilNode::SourceLeavesBattlefield => {
                json!({ "kind": "sourceLeavesBattlefield" })
            }
        };
    }
    if matches!(
        effect.regeneration,
        ZoneChangeRegenerationPolicy::Prohibited { .. }
    ) {
        operation["cannotRegenerate"] = Value::Bool(true);
    }
    Ok(operation)
}

pub(in crate::oracle::canonical) fn lower_targeted_zone_change_effect(
    action: ZoneChangeAction,
    target: Value,
) -> Value {
    match action {
        ZoneChangeAction::Destroy => json!({
            "kind": "destroyPermanent",
            "permanent": target,
        }),
        ZoneChangeAction::Exile => json!({
            "kind": "exilePermanent",
            "permanent": target,
        }),
        ZoneChangeAction::Move(ZoneChangeDestination::OwnerHand) => json!({
            "kind": "returnToOwnersHand",
            "object": target,
        }),
        ZoneChangeAction::Move(ZoneChangeDestination::OwnerLibrary(position)) => json!({
            "kind": "putPermanentOnOwnerLibrary",
            "permanent": target,
            "position": match position {
                LibraryPosition::Top => "top",
                LibraryPosition::Bottom => "bottom",
            },
        }),
    }
}

fn target_contains_batch(target: &TargetListNode, expected: CharacteristicBatch) -> bool {
    target.slots.iter().any(|slot| match &slot.selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate.contains_characteristic_batch(expected)
        }
        TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_)
        | TargetSelector::StackItem(_) => false,
    })
}

#[cfg(test)]
#[path = "plural_zone_change_tests.rs"]
mod plural_tests;

/// `exile all graveyards`, `exile each opponent's graveyard`: every card in
/// the graveyards of the scoped players moves to exile, each to its owner's
/// (CR 400.3, 406.2). The shared card mover and zone query carry it; the
/// population is the shared player-set reference, read on resolution.
pub(in crate::oracle::canonical) fn lower_zone_batch_effect(effect: &ZoneBatchEffectNode) -> Value {
    let players = lower_player_set_reference(match effect.collection {
        ZoneCollectionNode::AllGraveyards => PlayerSetReferenceNode::EachPlayer,
        ZoneCollectionNode::OpponentGraveyards => {
            PlayerSetReferenceNode::OpponentsOfAbilityController
        }
    });
    let cards = json!({
        "kind": "cardsInZone",
        "zone": { "kind": "graveyard", "players": players },
    });
    match effect.action {
        ZoneBatchActionNode::Exile => json!({
            "kind": "moveCards",
            "cards": cards,
            "to": {
                "kind": "exile",
                "player": { "kind": "ownerOf", "object": { "kind": "candidate" } },
                "faceDown": false,
            },
        }),
    }
}

/// `shuffle X into a library` puts X into that library, then shuffles it
/// (CR 701.24b). The shuffle names the moved cards, not a player: each
/// library that received one of them is shuffled once, whoever owns it.
pub(in crate::oracle::canonical) fn lower_shuffle_libraries_holding(cards: Value) -> Value {
    json!({ "kind": "shuffleLibrariesHolding", "cards": cards })
}

/// The permanent an entering Aura, Equipment or Fortification is attached
/// to (CR 303.4f, 301.5c): the source, a bound reference, one declared
/// target of the enclosing ability, or one permanent chosen on resolution
/// (the choice is emitted before the entry). `id` names the declaration or
/// the choice; the caller attaches the entered object to the result.
pub(in crate::oracle::canonical) fn lower_entry_attachment_recipient(
    cursor: &OracleTokenCursor<'_>,
    attachment: &EntryAttachmentNode,
    id: &str,
    declarations: TargetlessDeclarationSupport,
    effects: &mut Vec<Value>,
    target_decisions: &mut Vec<Value>,
) -> MachineResult<Value> {
    match attachment {
        EntryAttachmentNode::Source => Ok(self_ref()),
        EntryAttachmentNode::Reference(reference) => {
            bound_permanent_reference_value(*reference, &[]).ok_or_else(|| {
                cursor.fail("an attachment reference with a value without a target binding")
            })
        }
        EntryAttachmentNode::Target(targets) => {
            let [slot] = targets.slots.as_slice() else {
                return Err(cursor.fail("one attachment target slot"));
            };
            let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                .ok_or_else(|| cursor.fail("a permanent target selector for the attachment"))?;
            target_decisions.push(target_decision(id, candidates, 1, 1));
            Ok(chosen_target(id))
        }
        EntryAttachmentNode::Chosen(selection) => {
            let (predicate, controller_filter) =
                lower_permanent_query_in_context(&selection.query, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
            let mut candidates = json!({ "kind": "permanents", "where": predicate });
            match controller_filter {
                Some(LoweredQueryController::Player(player)) => candidates["controller"] = player,
                Some(LoweredQueryController::Population(_)) => {
                    return Err(cursor.fail(
                        TargetlessLoweringError::UnsupportedCollectionController.expectation(),
                    ));
                }
                None => {}
            }
            if selection.query.exclude_source {
                candidates["excludeSource"] = json!(true);
            }
            effects.push(json!({
                "kind": "choosePermanents",
                "id": id,
                "player": controller(),
                "candidates": candidates,
                "minimum": integer(1),
                "maximum": integer(1),
            }));
            Ok(decision_result(id))
        }
    }
}
