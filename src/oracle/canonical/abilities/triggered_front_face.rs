use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

fn recognize_optional_investigate_reminder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    if cursor.peek().is_none() {
        return Ok(());
    }
    cursor.enter("reminder.investigate.open");
    cursor.consume("(")?;
    cursor.enter("reminder.investigate.create");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Create,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    cursor.enter("reminder.investigate.quantity");
    cursor.consume("a")?;
    cursor.enter("reminder.investigate.token-name");
    cursor.consume("clue")?;
    cursor.consume("token")?;
    cursor.consume(".")?;
    cursor.enter("reminder.investigate.token-definition");
    cursor.consume("it's")?;
    cursor.consume("an")?;
    cursor.consume("artifact")?;
    cursor.consume("with")?;
    cursor.consume("\"")?;
    cursor.enter("reminder.investigate.activation-cost");
    cursor.consume("{2}")?;
    cursor.consume(",")?;
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Sacrifice,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    cursor.consume("this")?;
    cursor.consume("token")?;
    cursor.consume(":")?;
    cursor.enter("reminder.investigate.activation-effect");
    cursor.consume("draw")?;
    cursor.consume("a")?;
    cursor.consume("card")?;
    cursor.consume(".")?;
    cursor.consume("\"")?;
    cursor.enter("reminder.investigate.close");
    cursor.consume(")")?;
    Ok(())
}

pub(in crate::oracle::canonical) fn claims_promoted_attack_investigate_family(
    input: &AbilityInput<'_>,
) -> bool {
    let Some(separator) = input.tokens.iter().position(|token| token.raw == ",") else {
        return false;
    };
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("whenever"))
        && input.tokens[..separator]
            .last()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("attacks"))
        && input.tokens[separator + 1..]
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("investigate"))
}

pub(in crate::oracle::canonical) fn parse_promoted_attack_investigate(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-attacks-investigate-trigger", input.tokens);
    cursor.enter("trigger.source-attacks-investigate.envelope");
    cursor.consume("whenever")?;
    let event = recognize_source_attacked_event(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.source-attacks-investigate.separator");
    cursor.consume(",")?;
    let effect = recognize_investigate_effect(&mut cursor)?;
    cursor.enter("trigger.source-attacks-investigate.terminator");
    cursor.consume(".")?;

    recognize_optional_investigate_reminder(&mut cursor)?;
    cursor.finish()?;

    cursor.enter("trigger.source-attacks-investigate.validation");
    cursor.require(
        event
            == LocalTriggerEventNode::SourceAttacked {
                source: LocalObjectReference::SourceSelf,
            },
        "the current source attacking",
    )?;
    cursor.require(
        effect.player == LocalPlayerReference::Controller,
        "the source controller investigating",
    )?;

    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "declaredAttacker", "object": self_ref() },
            "effects": [{
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": { "kind": "namedToken", "name": "Clue" },
            }],
        }),
        &[
            "Recognize the source-attacked event through the shared event automaton",
            "Recognize investigate as one independent controller effect",
            "Lower investigate to the reusable named-token creation primitive",
        ],
    ))
}

pub(in crate::oracle::canonical) fn claims_promoted_ordinal_draw_source_transform_family(
    input: &AbilityInput<'_>,
) -> bool {
    let Some(separator) = input.tokens.iter().position(|token| token.raw == ",") else {
        return false;
    };
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("when"))
        && input.tokens[..separator]
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("draw"))
        && input.tokens[separator + 1..]
            .first()
            .is_some_and(|token| is_verb_form(&token.raw, VerbLemma::Exile, VerbForm::Base))
}

pub(in crate::oracle::canonical) fn parse_promoted_ordinal_draw_source_transform(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("ordinal-draw-source-transform-trigger", input.tokens);
    cursor.enter("trigger.ordinal-draw-source-transform.envelope");
    cursor.consume("when")?;
    let event = recognize_ordinal_card_drawn_event(&mut cursor)?;
    cursor.enter("trigger.ordinal-draw-source-transform.separator");
    cursor.consume(",")?;
    let context = EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &context,
    )?;

    cursor.enter("trigger.ordinal-draw-source-transform.validation");
    let LocalTriggerEventNode::OrdinalCardDrawn {
        player,
        ordinal,
        period: DrawPeriod::Turn,
    } = event
    else {
        return Err(cursor.fail("an ordinal card draw during a turn"));
    };
    cursor.require(
        player == LocalPlayerReference::Controller,
        "the source controller drawing",
    )?;
    let [
        LocalEffectNode::MoveSource(exile),
        LocalEffectNode::MoveSource(returned),
    ] = sequence.effects.as_slice()
    else {
        return Err(cursor.fail("two independent source-zone moves"));
    };
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "a then-sequenced source return",
    )?;
    cursor.require(
        exile.source == LocalObjectReference::SourceSelf
            && exile.from == SourceZone::Battlefield
            && exile.to == SourceZone::Exile
            && exile.face_change == SourceFaceChange::Preserve,
        "a source move from battlefield to exile",
    )?;
    cursor.require(
        returned.source == LocalObjectReference::SourceSelf
            && returned.from == SourceZone::Exile
            && returned.to == SourceZone::Battlefield
            && returned.face_change == SourceFaceChange::Transform
            && returned.controller == SourceMoveController::Owner,
        "the same source returning transformed under owner control",
    )?;

    let binding = "exiledSource";
    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "cardDrawn",
                "player": controller(),
                "drawOrdinal": { "kind": "integer", "value": ordinal },
            },
            "effects": [
                lower_source_move_effect(exile, self_ref(), Some(binding)),
                lower_source_move_effect(
                    returned,
                    json!({ "kind": "boundObjects", "binding": binding }),
                    None,
                ),
            ],
        }),
        &[
            "Recognize ordinal card draw through the shared event automaton",
            "Resolve the printed source name and return anaphor to SourceSelf",
            "Compose two independently observable source-zone moves",
            "Bind the object moved to exile before returning it transformed",
        ],
    ))
}
