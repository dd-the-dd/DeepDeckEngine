use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str =
    "Flying\nDiscard this card: Exile up to two target cards from graveyards.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Faerie Macabre".to_string(),
        type_line: "Creature — Faerie Rogue".to_string(),
        mana_cost: Some("{1}{B}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn exact_card_is_fully_native_and_lowers_to_independent_primitives() {
    let parsed = parse(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["kind"],
        "keywordAbility"
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["ability"]["kind"],
        "flying"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "activationZone": "hand",
            "costs": [{
                "kind": "discardCard",
                "card": { "kind": "self" },
            }],
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetGraveyardCard",
                    "kind": "chooseTargets",
                    "minimum": 0,
                    "maximum": 2,
                    "candidates": {
                        "kind": "cards",
                        "zone": { "kind": "anyGraveyard" },
                        "where": null,
                    },
                }],
            },
            "effects": [{
                "kind": "exileTargetCards",
                "cards": {
                    "kind": "chosenTargets",
                    "id": "targetGraveyardCard",
                },
            }],
        }))
    );
}

#[test]
fn exact_target_count_and_an_independent_cost_reuse_the_same_children() {
    let parsed = parse("Pay 2 life: Exile three target cards from any graveyard.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling emits a rule");
    assert_eq!(rule["costs"][0]["kind"], "payLife");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 3);
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"], 3);
    assert_eq!(rule["effects"][0]["kind"], "exileTargetCards");
    assert!(rule.get("activationZone").is_none());
}

#[test]
fn existing_single_card_form_keeps_its_source_linked_primitive() {
    // `exileTargetCardWithSource` is a retired composite (contract fixture
    // retired-exile-composites). Without a linked reader on the face
    // (CR 607), the single target lowers to the ordinary exile action.
    let parsed = parse("{T}: Exile target card from any graveyard.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(parsed.status, "canonical");
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"],
        json!([{
            "kind": "exileTargetCards",
            "cards": { "kind": "chosenTargets", "id": "targetCard" },
        }])
    );
    assert_eq!(
        ability.rule.as_ref().unwrap()["declaration"]["decisions"][0]["maximum"],
        1
    );
}

#[test]
fn multiple_targets_from_a_single_graveyard_keep_the_group_constraint() {
    let parsed = parse("Discard this card: Exile up to two target cards from a single graveyard.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref().unwrap()["declaration"]["decisions"][0]["selectionConstraint"],
        json!({ "kind": "sameZoneOwner", "zone": "graveyard" })
    );
}

#[test]
fn neighboring_battlefield_zone_change_family_is_not_stolen() {
    let parsed = parse("{T}: Exile target creature.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"][0]["kind"],
        "exilePermanent"
    );
}

#[test]
fn discard_another_card_cost_is_not_stolen_by_the_discard_self_family() {
    // The shared payment reader now composes `discard a card` as an ordinary
    // hand-card cost. The discard-self family never claims it: the cost is a
    // chosen hand card, not this card, and the ability is not activated from
    // the hand.
    let parsed = parse("Discard a card: Exile target card from any graveyard.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.diagnostics.is_empty());
    let rule = ability
        .rule
        .as_ref()
        .expect("the shared program emits a rule");
    assert!(rule.get("activationZone").is_none());
    assert_eq!(
        rule["costs"],
        json!([{
            "kind": "discardCard",
            "card": { "kind": "chosenObject", "id": "activationCostCard0" },
        }])
    );
    assert_eq!(
        rule["declaration"]["decisions"],
        json!([
            {
                "id": "activationCostCard0",
                "kind": "chooseObjects",
                "quantity": {
                    "kind": "exactly",
                    "value": { "kind": "integer", "value": 1 },
                },
                "candidates": {
                    "kind": "cards",
                    "zone": {
                        "kind": "hand",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    },
                    "where": null,
                },
            },
            {
                "id": "targetCard",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "cards",
                    "zone": { "kind": "anyGraveyard" },
                    "where": null,
                },
            },
        ])
    );
    assert_eq!(
        rule["effects"],
        json!([{
            "kind": "exileTargetCards",
            "cards": { "kind": "chosenTargets", "id": "targetCard" },
        }])
    );
}

#[test]
fn claimed_near_misses_fail_at_precise_states_without_legacy_fallback() {
    for (oracle_text, automaton, state, found) in [
        (
            "Discard this card: Exile up to two target cards from neighboring graveyards.",
            "activated-card-zone-change",
            "card-target.source-owner",
            "neighboring",
        ),
        (
            "Discard this card: Exile up to two target cards from graveyards instead.",
            "contextual-effect-program",
            "sequence.connector-or-end",
            "instead",
        ),
        // An X with no producer on this face is a binding failure, never an
        // invented `chooseNumber` declaration.
        (
            "Discard this card: Exile up to X target cards from graveyards.",
            "card-context-binding",
            "binding.x-value.source",
            "{1}{B}{B}",
        ),
    ] {
        let parsed = parse(oracle_text);
        let ability = &parsed.abilities[0];

        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_eq!(ability.status, "unsupported", "{oracle_text}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some(automaton),
            "{oracle_text}"
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state), "{oracle_text}");
        assert_eq!(diagnostic.found.as_deref(), Some(found), "{oracle_text}");
    }
}

#[test]
#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Faerie Macabre missing-colon diagnostic; see docs/coverage-targets.md"
)]
fn missing_colon_activation_fails_at_the_family_separator() {
    let parsed = parse("Discard this card Exile up to two target cards from graveyards.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("activated-card-zone-change")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("activation.card-zone-change.effect-separator")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("Exile"));
}

#[test]
fn exact_oracle_has_no_unknown_tokens() {
    let parsed = parse(EXACT_ORACLE);
    let unknown = parsed
        .abilities
        .iter()
        .flat_map(|ability| &ability.entities)
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
