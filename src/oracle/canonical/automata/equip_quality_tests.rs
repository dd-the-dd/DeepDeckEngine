//! CR 702.6c restricted Equip: `Equip [quality] creature [cost]`.
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

use super::super::ObjectPredicate;
use super::{OracleTokenCursor, recognize_mana_equip};

fn quality(text: &str) -> super::super::super::MachineResult<Option<ObjectPredicate>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("equip-quality-test", &tokens);
    recognize_mana_equip(&mut cursor).map(|node| node.quality)
}

#[test]
fn blackblade_restricted_equip_lowers_a_target_filter() {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: "Blackblade Reforged".to_string(),
        type_line: "Legendary Artifact — Equipment".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(
            "Equipped creature gets +1/+1 for each land you control.\nEquip legendary creature {3}\nEquip {7}"
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    let rule = result.abilities[1]
        .rule
        .as_ref()
        .unwrap_or_else(|| panic!("{:?}", result.abilities[1].diagnostics));
    assert_eq!(rule["ability"]["kind"], "equip");
    assert_eq!(rule["ability"]["costs"][0]["manaCost"], "{3}");
    let filter = rule["ability"]["where"].to_string();
    assert!(
        filter.contains("Creature") && filter.to_ascii_lowercase().contains("legendary"),
        "{filter}"
    );
    let unrestricted = result.abilities[2].rule.as_ref().expect("Equip {7}");
    assert!(unrestricted["ability"].get("where").is_none());
}

#[test]
fn other_qualities_reuse_the_shared_atom_reader() {
    // Unused combinations: a subtype and a color quality.
    assert!(matches!(quality("Equip Knight creature {1}"), Ok(Some(_))));
    assert!(matches!(quality("Equip white creature {2}"), Ok(Some(_))));
    assert!(matches!(quality("Equip {2}"), Ok(None)));
}

#[test]
fn restricted_equip_near_misses_fail_explicitly() {
    assert!(quality("Equip legendary creature").is_err(), "no cost");
    assert!(
        quality("Equip legendary creature {3} ({3}: Attach to target creature you control. Equip only as a sorcery.)")
            .is_err(),
        "the unrestricted reminder contradicts the quality"
    );
    assert!(
        quality("Equip planeswalker {1}").is_err(),
        "CR 702.6e is a different variant"
    );
}
