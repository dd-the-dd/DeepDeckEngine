use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

/// Claims a targeted battlefield zone change immediately followed by a
/// controller-bound creature or catalog-named token creation. The child automata own
/// targets, anaphora, quantities, characteristics, and full consumption.
pub(in crate::oracle::canonical) fn claims_promoted_zone_change_token_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.first().is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("destroy")
            || is_verb_form(&token.raw, VerbLemma::Exile, VerbForm::Base)
    }) && tokens.iter().enumerate().any(|(index, token)| {
        token.raw == "."
            && controller_reference_head_length(&tokens[index + 1..]).is_some_and(|length| {
                tokens.get(index + 1 + length).is_some_and(|action| {
                    is_verb_form(
                        &action.raw,
                        VerbLemma::Create,
                        VerbForm::ThirdPersonSingular,
                    )
                })
            })
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_zone_change_token(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("zone-change-create-token", input.tokens);
    let reference_context = EffectRecognitionContext::default()
        .with_target_reference_support(TargetReferenceSupport::TypedNominal);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
        &reference_context,
    )?;
    cursor.enter("sequence.zone-change-token.validation");
    let [LocalEffectNode::ZoneChange(zone_change), creation] = sequence.effects.as_slice() else {
        return Err(cursor.fail("a zone change followed by creature-token creation"));
    };
    let controller_binding_id = "changedPermanentController";
    let token_controller = json!({"kind":"boundValue","id":controller_binding_id});
    let (player, token_effect) = match creation {
        LocalEffectNode::CreateTokens(create_tokens) => (
            &create_tokens.player,
            json!({
                "kind":"createTokens", "controller":token_controller,
                "quantity":lower_numeric_atom(create_tokens.quantity.clone()),
                "token":{
                "colors": create_tokens
                    .token
                    .colors
                    .iter()
                    .map(|color| canonical_color_name(*color))
                    .collect::<Vec<_>>(),
                "types": ["Creature"],
                "subtypes": create_tokens
                    .token
                    .subtypes
                    .iter()
                    .map(|subtype| subtype.ir_name())
                    .collect::<Vec<_>>(),
                "power": create_tokens.token.power,
                "toughness": create_tokens.token.toughness,
                "abilities": [],
                }
            }),
        ),
        LocalEffectNode::CreateNamedTokens(create_tokens) => (
            &create_tokens.player,
            lower_named_token_creation(
                &mut cursor,
                &create_tokens.creation,
                token_controller,
                true,
            )?,
        ),
        _ => return Err(cursor.fail("a zone change followed by token creation")),
    };
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "one inter-sentence effect sequence",
    )?;
    let player_binding = match player {
        LocalPlayerReference::ControllerOf(binding) => *binding,
        LocalPlayerReference::ControllerOfQualified(reference) => {
            cursor.enter("sequence.zone-change-token.controller-nominal");
            cursor.require(
                reference.binding == binding,
                "the prior target binding for the qualified controller",
            )?;
            let nominal = cursor.require_value(
                reference.required_nominal,
                "a retained qualified controller nominal",
            )?;
            let slot = &zone_change.target.slots[0];
            cursor.require(
                selector_implies_nominal(&slot.selector, &nominal.as_predicate()),
                "a controller reference nominal guaranteed by the original target selector",
            )?;
            reference.binding
        }
        LocalPlayerReference::Controller | LocalPlayerReference::Bound(_) => {
            return Err(cursor.fail("the prior target's controller as token controller"));
        }
    };
    cursor.require(
        player_binding == binding,
        "the prior target's controller as token controller",
    )?;

    let (target_id, candidates, minimum, maximum) = lower_single_target(&zone_change.target);
    let target = chosen_target(target_id);
    let zone_change_effect =
        lower_targeted_zone_change_node(&mut cursor, zone_change, target.clone())?;
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
                "decisions": [target_decision(
                    target_id,
                    candidates,
                    minimum,
                    maximum,
                )],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": controller_binding_id,
                    "value": { "kind": "controllerOf", "object": target.clone() },
                },
                zone_change_effect,
                token_effect,
            ],
        }),
        &[
            "Recognize one targeted zone change and one controller-bound token creation",
            "Resolve the token controller through the prior target binding",
            "Snapshot the controller before the permanent leaves the battlefield",
            "Lower binding, zone change, and token creation as independent primitives",
            &trace,
        ],
    ))
}
