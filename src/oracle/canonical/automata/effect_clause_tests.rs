//! The effect role reads lexicon lemmas through the shared clause. `put`
//! carries two frames: the counters frame and the object-to-zone frame; the
//! clause picks the zone frame transactionally for a target permanent put
//! into its owner's library (CR 401.4), the counters frame otherwise.
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};
use serde_json::{Value, json};

fn effects(type_line: &str, text: &str) -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Put Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.abilities
    );
    parsed.abilities[0].rule.clone().expect("rule")["effects"].clone()
}

#[test]
fn put_chooses_the_zone_frame_or_the_counters_frame_from_its_operand() {
    // Grasp of Phantoms / Condemn shapes: the library position is typed.
    for (text, position) in [
        ("Put target creature on top of its owner's library.", "top"),
        (
            "Put target attacking creature on the bottom of its owner's library.",
            "bottom",
        ),
    ] {
        let effects = effects("Instant", text);
        assert_eq!(
            effects[0]["kind"], "putPermanentOnOwnerLibrary",
            "{effects:#}"
        );
        assert_eq!(effects[0]["position"], json!(position));
        assert_eq!(effects[0]["permanent"]["kind"], "chosenTarget");
    }
    // Unused combination: a triggered body with a nonland-permanent target.
    let rule = effects(
        "Creature — Vedalken",
        "When this creature enters, put target nonland permanent on top of its owner's library.",
    );
    assert_eq!(rule[0]["kind"], "putPermanentOnOwnerLibrary", "{rule:#}");
    // The same lemma with a counter noun keeps the counters frame.
    let counters = effects("Instant", "Put a +1/+1 counter on target creature.");
    assert_eq!(counters[0]["kind"], "putCounters", "{counters:#}");
}

#[test]
fn put_zone_frame_near_misses_fail_explicitly() {
    for text in [
        "Put two target creatures on top of their owners' libraries.",
        // `on top of your library` is the owner's library (CR 400.3) and
        // now reads as such; a separately declared library holder is not.
        "Put target creature on top of target player's library.",
        "Put target creature into its owner's library.",
    ] {
        let parsed = parse_oracle_card(OracleCardParseRequest {
            card_name: "Put Near Miss".to_string(),
            type_line: "Instant".to_string(),
            mana_cost: Some("{1}{U}".to_string()),
            oracle_text: Some(text.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        });
        assert_ne!(parsed.status, "canonical", "{text}");
    }
}
