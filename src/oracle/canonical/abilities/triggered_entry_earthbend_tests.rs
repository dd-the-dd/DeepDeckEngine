use crate::oracle::canonical::{CanonicalParseOutcome, parse_canonical_rule};
use crate::oracle::model::{AbilitySource, RecognizerProvenance};
use crate::oracle::syntax::{AbilityInput, NameLinkKind, ResolvedNameLink, recognize_entities};

use super::{
    claims_promoted_source_entry_earthbend_sequence_family,
    parse_promoted_source_entry_earthbend_sequence,
};

fn with_input<T>(text: &str, run: impl FnOnce(AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    run(AbilityInput {
        face_name: "Test Ascension",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn exact_entry_sequence_lowers_only_to_primitive_effects() {
    let text = "When this enchantment enters, earthbend 2. Then search your library for a basic land card, put it onto the battlefield tapped, then shuffle.";
    let draft = with_input(text, |input| {
        assert!(claims_promoted_source_entry_earthbend_sequence_family(
            &input
        ));
        match parse_canonical_rule(&input, "triggeredAbility") {
            CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            } => draft,
            _ => panic!("exact sequence must dispatch through the native recognizer"),
        }
    });

    assert_eq!(draft.rule["kind"], "triggeredAbility");
    let decision = &draft.rule["declaration"]["decisions"][0];
    assert_eq!(decision["id"], "earthbendLand0");
    assert_eq!(decision["kind"], "chooseTargets");
    assert_eq!(decision["minimum"], 1);
    assert_eq!(decision["maximum"], 1);
    assert_eq!(decision["candidates"]["kind"], "permanents");
    assert_eq!(decision["candidates"]["controller"]["kind"], "controllerOf");
    assert_eq!(
        decision["candidates"]["controller"]["object"]["kind"],
        "self"
    );
    assert_eq!(decision["candidates"]["where"]["kind"], "cardTypeContains");
    assert_eq!(decision["candidates"]["where"]["value"], "Land");
    let effects = draft.rule["effects"].as_array().expect("effect array");
    assert_eq!(effects.len(), 9);
    assert_eq!(effects[0]["kind"], "becomeCreature");
    assert_eq!(effects[1]["kind"], "grantKeyword");
    assert_eq!(effects[2]["kind"], "putCounters");
    assert_eq!(effects[3]["kind"], "installDelayedZoneChangeTrigger");
    assert_eq!(effects[4]["kind"], "emitAbilitySignal");
    assert_eq!(effects[5]["kind"], "chooseCards");
    assert_eq!(effects[6]["kind"], "moveCards");
    assert_eq!(effects[7]["kind"], "shuffleZone");
    assert_eq!(effects[8]["kind"], "recordLibrarySearch");
    assert!(!draft.rule.to_string().contains("\"kind\":\"earthbend\""));
    assert_eq!(effects[2]["count"]["value"], 2);
    assert_eq!(effects[6]["to"]["tapped"], true);
    assert_eq!(effects[5]["candidates"]["where"]["kind"], "and");
}

#[test]
fn sibling_varies_source_quantity_selector_reference_and_entry_state() {
    let text = "When this artifact enters, earthbend 3. Then search your library for a Forest card, put that card onto the battlefield, then shuffle.";
    let draft = with_input(text, |input| {
        parse_promoted_source_entry_earthbend_sequence(&input).expect("sibling parses")
    });
    let effects = draft.rule["effects"].as_array().expect("effect array");
    assert_eq!(effects[2]["count"]["value"], 3);
    assert_eq!(effects[5]["candidates"]["where"]["kind"], "subtypeContains");
    assert_eq!(effects[5]["candidates"]["where"]["value"], "Forest");
    assert_eq!(effects[6]["to"]["tapped"], false);
}

#[test]
fn missing_card_noun_is_a_committed_selector_failure() {
    let text = "When this enchantment enters, earthbend 2. Then search your library for a basic land, put it onto the battlefield tapped, then shuffle.";
    let failure = with_input(text, |input| {
        assert!(claims_promoted_source_entry_earthbend_sequence_family(
            &input
        ));
        let Err(failure) = parse_promoted_source_entry_earthbend_sequence(&input) else {
            panic!("near miss must fail");
        };
        failure
    });
    assert_eq!(failure.state, "selector.search-card.card-type-branch");
    assert_eq!(failure.actual.as_deref(), Some("put"));
}

#[test]
fn unbound_x_is_rejected_by_the_parent_context() {
    let text = "When this enchantment enters, earthbend X. Then search your library for a basic land card, put it onto the battlefield tapped, then shuffle.";
    let failure = with_input(text, |input| {
        let Err(failure) = parse_promoted_source_entry_earthbend_sequence(&input) else {
            panic!("unbound X must fail");
        };
        failure
    });
    assert_eq!(failure.state, "earthbend.action.variable-binding");
}

#[test]
fn claim_rejects_an_unrelated_land_entry_event() {
    let text = "When a land enters, earthbend 2. Then search your library for a basic land card, put it onto the battlefield tapped, then shuffle.";
    with_input(text, |input| {
        assert!(!claims_promoted_source_entry_earthbend_sequence_family(
            &input
        ));
    });
}

#[test]
fn claim_accepts_a_current_source_resolved_by_name_link() {
    let text = "When Test Ascension enters, earthbend 2. Then search your library for a basic land card, put it onto the battlefield tapped, then shuffle.";
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    let links = [ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: "Test Ascension".to_string(),
        raw: "Test Ascension".to_string(),
        token_start: 1,
        token_end: 3,
        span_start: tokens[1].span_start,
        span_end: tokens[2].span_end,
    }];
    let input = AbilityInput {
        face_name: "Test Ascension",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &links,
        name_resolution_failure: None,
    };
    assert!(claims_promoted_source_entry_earthbend_sequence_family(
        &input
    ));
}
