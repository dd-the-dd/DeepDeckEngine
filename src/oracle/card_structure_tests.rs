use super::*;
use crate::oracle::{OracleCardFace, parse_oracle_card_native};

fn request(layout: Option<&str>, types: &[&str]) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Structure Probe".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: None,
        oracle_text: Some("Draw a card.".to_string()),
        layout: layout.map(ToOwned::to_owned),
        faces: types
            .iter()
            .enumerate()
            .map(|(index, type_line)| OracleCardFace {
                id: format!("part-{index}"),
                name: format!("Part {index}"),
                type_line: type_line.to_string(),
                mana_cost: Some("{1}".to_string()),
                oracle_text: "Draw a card.".to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            })
            .collect(),
    }
}

#[test]
fn structural_families_preserve_part_identities_and_roles() {
    let cases: &[(Option<&str>, &[&str], CardStructureKind)] = &[
        (None, &[], CardStructureKind::Single),
        (Some("normal"), &[], CardStructureKind::Single),
        (Some("class"), &[], CardStructureKind::Single),
        (Some("meld"), &[], CardStructureKind::MeldComponent),
        (
            Some("transform"),
            &["Creature", "Planeswalker"],
            CardStructureKind::Transform,
        ),
        (
            Some("modal_dfc"),
            &["Instant", "Land"],
            CardStructureKind::ModalDoubleFaced,
        ),
        (
            Some("flip"),
            &["Creature", "Enchantment"],
            CardStructureKind::Flip,
        ),
        (
            Some("split"),
            &["Instant", "Sorcery"],
            CardStructureKind::Split,
        ),
        (
            Some("split"),
            &["Instant", "Sorcery", "Instant"],
            CardStructureKind::Split,
        ),
        (
            Some("split"),
            &["Enchantment — Room", "Enchantment — Room"],
            CardStructureKind::Rooms,
        ),
        (
            Some("adventure"),
            &["Creature", "Instant — Adventure"],
            CardStructureKind::Adventure,
        ),
        (
            Some("adventure"),
            &["Sorcery", "Sorcery — Adventure"],
            CardStructureKind::Adventure,
        ),
        (
            Some("adventure"),
            &["Creature", "Sorcery — Omen"],
            CardStructureKind::Omen,
        ),
        (
            Some("prepare"),
            &["Creature", "Instant"],
            CardStructureKind::Preparation,
        ),
        (
            Some("double_faced_token"),
            &["Token Creature", "Token Creature"],
            CardStructureKind::DoubleFacedToken,
        ),
        (
            Some("art_series"),
            &["Card", "Card"],
            CardStructureKind::ArtSeries,
        ),
    ];
    for &(layout, types, expected) in cases {
        let input = request(layout, types);
        let plan = plan_card_structure(&input).expect("registered structural family");
        assert_eq!(plan.kind, expected, "{layout:?}");
        assert_eq!(plan.parts.len(), types.len().max(1));
        for (index, part) in plan.parts.iter().enumerate().filter(|_| !types.is_empty()) {
            assert_eq!(part.face_index, Some(index));
            assert_eq!(
                part.face_id.as_deref(),
                Some(input.faces[index].id.as_str())
            );
        }
    }
    let omen =
        plan_card_structure(&request(Some("adventure"), &["Creature", "Instant — Omen"])).unwrap();
    assert_eq!(omen.parts[1].role, CardPartRole::OmenSpell);
    let adventure = plan_card_structure(&request(
        Some("adventure"),
        &["Creature", "Instant — Adventure"],
    ))
    .unwrap();
    assert_eq!(adventure.parts[1].role, CardPartRole::AdventureSpell);
}

#[test]
fn unknown_or_ambiguous_formats_fail_before_oracle_recognition() {
    for input in [
        request(Some("future_three_sided"), &[]),
        request(None, &["Instant", "Sorcery"]),
        request(Some("adventure"), &["Creature", "Sorcery — Vengeance"]),
    ] {
        let parsed = parse_oracle_card_native(input);
        assert_eq!(parsed.status, "unsupported");
        assert!(parsed.card_structure.is_none());
        assert!(
            parsed.abilities.is_empty(),
            "do not start an ability automaton"
        );
        assert_eq!(parsed.diagnostics[0].code, "card_face_factory_missing");
        assert_eq!(
            parsed.diagnostics[0].automaton.as_deref(),
            Some("card-face-factory")
        );
    }
}

#[test]
fn malformed_known_formats_have_a_distinct_metadata_diagnostic() {
    for input in [
        request(Some("transform"), &["Creature"]),
        request(Some("normal"), &["Instant", "Sorcery"]),
        request(Some("prepare"), &["Creature", "Land"]),
        request(Some("adventure"), &["Creature", "Instant — Adventure Omen"]),
        request(Some("split"), &["Instant"]),
    ] {
        assert_eq!(
            plan_card_structure(&input).unwrap_err().code,
            "card_face_structure_invalid"
        );
    }
    let mut duplicate = request(Some("split"), &["Instant", "Sorcery"]);
    duplicate.faces[1].id = duplicate.faces[0].id.clone();
    assert_eq!(
        plan_card_structure(&duplicate).unwrap_err().state,
        "structure.part-identity"
    );
}

#[test]
fn valid_parts_reach_the_same_oracle_automata_with_their_own_context() {
    let parsed = parse_oracle_card_native(request(Some("split"), &["Instant", "Sorcery"]));
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert_eq!(
        parsed.abilities[0].source.face_id.as_deref(),
        Some("part-0")
    );
    assert_eq!(
        parsed.abilities[1].source.face_id.as_deref(),
        Some("part-1")
    );
    for ability in parsed.abilities {
        assert_eq!(ability.rule.unwrap()["effects"][0]["kind"], "drawCards");
    }
}
