use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities, recognize_oracle_entities};

fn parse_costs(text: &str) -> Result<ActivationCostSequenceNode, MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("activation-graveyard-exile-cost-test", &tokens);
    let costs = recognize_activation_cost_sequence(&mut cursor)?;
    cursor.finish()?;
    Ok(costs)
}

fn parse_activation(type_line: &str, text: &str) -> EffectProgramParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_contextual_effect_program(&AbilityInput {
        face_name: "Generic Program Source",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parsed_rule(type_line: &str, text: &str) -> Value {
    let attempt = parse_activation(type_line, text);
    let EffectProgramParseAttempt::Parsed(parsed) = attempt else {
        panic!(
            "expected a complete activated program: {text}: {:?}",
            match &attempt {
                EffectProgramParseAttempt::FailedSupport(f)
                | EffectProgramParseAttempt::Failed(f)
                | EffectProgramParseAttempt::Unpromoted(f) => format!("{f:?}"),
                _ => "other".into(),
            }
        );
    };
    parsed.rule
}

#[test]
fn exile_self_cost_makes_the_ability_graveyard_activated() {
    let rule = parsed_rule(
        "Artifact Creature - Construct",
        "{2}{B}, Exile this card from your graveyard: You draw a card and lose 1 life.",
    );
    assert_eq!(rule["kind"], "activatedAbility");
    assert_eq!(rule["activationZone"], "graveyard");
    assert_eq!(rule["costs"][0]["kind"], "payMana");
    assert_eq!(
        rule["costs"][1],
        json!({ "kind": "exileSource", "object": self_ref(), "zone": "graveyard" })
    );
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
}

#[test]
fn exile_self_cost_composes_with_other_costs_and_effects() {
    // A previously unused combination: life payment before the exile cost,
    // with a targeted effect that is not a draw.
    let rule = parsed_rule(
        "Creature - Spirit",
        "{W}, Pay 2 life, Exile this card from your graveyard: Target creature gains lifelink until end of turn.",
    );
    assert_eq!(rule["activationZone"], "graveyard");
    let kinds = rule["costs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cost| cost["kind"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["payMana", "payLife", "exileSource"]);
}

#[test]
fn exile_self_cost_requires_the_graveyard_source_zone_and_a_coherent_zone() {
    let hand = parse_costs("{R}, Exile this card from your hand").unwrap_err();
    assert_eq!(hand.state, "activation.exile-self.source-zone");
    let bare = parse_costs("{R}, Exile this card").unwrap_err();
    assert_eq!(bare.state, "activation.exile-self.origin");
    let tapped = parse_costs("{T}, Exile this card from your graveyard").unwrap();
    assert!(tapped.source_zone_requirements().battlefield);
    assert!(tapped.source_zone_requirements().graveyard);
    // A battlefield cost conflicts with the graveyard activation it proves.
    assert!(matches!(
        parse_activation(
            "Artifact",
            "{T}, Exile this card from your graveyard: Draw a card."
        ),
        EffectProgramParseAttempt::FailedSupport(_)
    ));
}

#[test]
fn returning_the_source_from_the_graveyard_proves_graveyard_activation() {
    let rule = parsed_rule(
        "Creature - Elemental",
        "{4}{G}: Return this card from your graveyard to your hand.",
    );
    assert_eq!(rule["activationZone"], "graveyard");
    assert_eq!(rule["costs"].as_array().unwrap().len(), 1);
    assert_eq!(
        rule["effects"][0],
        json!({
            "kind": "moveCards",
            "cards": self_ref(),
            "from": { "kind": "graveyard", "player": controller() },
            "to": { "kind": "hand", "player": controller() },
        })
    );
}

#[test]
fn graveyard_origin_composes_with_a_graveyard_exile_cost_and_other_effects() {
    // New combination: both proofs agree on the zone, and the return follows
    // an unrelated action in the same program.
    let rule = parsed_rule(
        "Creature - Spirit",
        "{1}{B}, Exile this card from your graveyard: You gain 2 life.",
    );
    assert_eq!(rule["activationZone"], "graveyard");
    let rule = parsed_rule(
        "Creature - Spirit",
        "{1}{B}: You gain 2 life, then return this card from your graveyard to your hand.",
    );
    assert_eq!(rule["activationZone"], "graveyard");
    assert_eq!(rule["effects"].as_array().unwrap().len(), 2);
}

#[test]
fn source_return_without_a_proved_origin_or_with_conflicting_zone_fails() {
    // No origin: bouncing from an unproved zone is not representable.
    assert!(!matches!(
        parse_activation("Creature", "{4}{G}: Return this card to your hand."),
        EffectProgramParseAttempt::Parsed(_)
    ));
    // A tap cost proves the battlefield, contradicting the graveyard origin.
    assert!(matches!(
        parse_activation(
            "Artifact",
            "{T}: Return this card from your graveyard to your hand."
        ),
        EffectProgramParseAttempt::FailedSupport(_)
    ));
    // Another zone or destination stays unsupported.
    assert!(!matches!(
        parse_activation(
            "Creature",
            "{4}{G}: Return this card from your graveyard to the battlefield."
        ),
        EffectProgramParseAttempt::Parsed(_)
    ));
}
