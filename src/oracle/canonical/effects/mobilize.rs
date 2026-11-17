use super::super::*;
use crate::oracle::ability_catalog::{
    CompiledLoweringAdapterId, CompiledRecognizerBindings, CompiledRecognizerId,
    compiled_integration_accepts_effects,
};
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_mobilize_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("mobilize"))
}

pub(in crate::oracle::canonical) fn parse_promoted_mobilize(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("mobilize", input.tokens);
    let mobilize =
        recognize_mobilize_keyword(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("keyword.mobilize.lower.quantity");
    let quantity = integer(mobilize.quantity);
    cursor.enter("keyword.mobilize.lower.primitive-sequence");
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": {
            "kind": "declaredAttacker",
            "object": self_ref(),
        },
        "effects": [
            {
                "kind": "createTokens",
                "controller": controller(),
                "quantity": quantity,
                "tapped": true,
                "attacking": true,
                "token": {
                    "types": ["Creature"],
                    "subtypes": ["Warrior"],
                    "colors": ["Red"],
                    "power": 1,
                    "toughness": 1,
                },
                "bind": "mobilizeTokens",
            },
            {
                "kind": "installDelayedStepTrigger",
                "controller": controller(),
                "step": "endStep",
                "captureBindings": ["mobilizeTokens"],
                "effects": [{
                    "kind": "sacrificePermanents",
                    "player": controller(),
                    "objects": {
                        "kind": "boundObjects",
                        "binding": "mobilizeTokens",
                    },
                }],
            },
        ],
    });
    cursor.enter("keyword.mobilize.lower.compiled-contract");
    cursor.require(
        compiled_integration_accepts_effects(
            CompiledRecognizerId::KeywordMobilizeV1,
            CompiledLoweringAdapterId::LowerKeywordMobilizeV1,
            CompiledRecognizerBindings::MobilizeV1 {
                quantity: mobilize.quantity,
            },
            rule["effects"]
                .as_array()
                .expect("Mobilize lowering always emits an effects array"),
        ),
        "the closed compiled Mobilize primitive contract",
    )?;

    Ok(draft(
        rule,
        &[
            "Recognize a positive literal Mobilize quantity",
            "Validate the optional attack, token-batch, and delayed-cleanup reminder",
            "Lower token creation and next-end-step cleanup as independent primitives",
            "Retain Mobilize identity in native recognizer provenance without emitting an action signal",
            &trace,
        ],
    ))
}
