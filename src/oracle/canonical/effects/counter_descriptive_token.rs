use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

/// Claims a counter followed by a controller-linked descriptive creature-token
/// sentence. The claim uses lexical structure only; the child machines retain
/// ownership of selector unions, anaphora, characteristics, and keywords.
pub(in crate::oracle::canonical) fn claims_promoted_counter_descriptive_token_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(boundary) = tokens.windows(4).position(|window| {
        window[0].raw == "."
            && window[1].raw.eq_ignore_ascii_case("its")
            && is_verb_form(
                &window[3].raw,
                VerbLemma::Create,
                VerbForm::ThirdPersonSingular,
            )
    }) else {
        return false;
    };
    let creation = &tokens[boundary + 4..];

    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && creation
            .iter()
            .any(|token| token.token_type == "counterValue" && token.raw.contains('/'))
        && creation
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("creature"))
        && creation.iter().any(|token| {
            token.raw.eq_ignore_ascii_case("token") || token.raw.eq_ignore_ascii_case("tokens")
        })
        && creation
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("with"))
}

pub(in crate::oracle::canonical) fn parse_promoted_counter_descriptive_token(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("counter-descriptive-token-sequence", input.tokens);
    let sequence = recognize_counter_descriptive_token_sequence(&mut cursor)?;
    cursor.enter("sequence.counter-descriptive-token.validation");

    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    cursor.require(
        sequence.token_creation.player == LocalPlayerReference::ControllerOf(binding),
        "the counter target's controller as token controller",
    )?;

    let (target_id, candidates, minimum, maximum) =
        lower_counter_spell_target(&cursor, &sequence.counter)?;
    let target = chosen_target(target_id);
    let controller_binding_id = "counteredSpellController";
    let token = &sequence.token_creation.token;
    let abilities = sequence
        .keywords
        .iter()
        .map(|keyword| json!({ "kind": keyword.canonical_name() }))
        .collect::<Vec<_>>();
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
                {
                    "kind": "counterSpell",
                    "spell": target,
                },
                {
                    "kind": "createTokens",
                    "controller": { "kind": "boundValue", "id": controller_binding_id },
                    "quantity": lower_numeric_atom(sequence.token_creation.quantity),
                    "token": {
                        "colors": token
                            .colors
                            .iter()
                            .map(|color| canonical_color_name(*color))
                            .collect::<Vec<_>>(),
                        "types": ["Creature"],
                        "subtypes": token
                            .subtypes
                            .iter()
                            .map(|subtype| subtype.ir_name())
                            .collect::<Vec<_>>(),
                        "power": token.power,
                        "toughness": token.toughness,
                        "abilities": abilities,
                    },
                },
            ],
        }),
        &[
            "Recognize counter selection and descriptive token creation as independent effects",
            "Resolve `its controller` through the prior target binding",
            "Compose token statistics, color, subtype, and keyword children",
            "Snapshot the target controller before attempting to counter the spell",
            "Lower to bind, counterSpell, and createTokens primitives",
            &trace,
        ],
    ))
}
