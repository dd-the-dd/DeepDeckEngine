use deepdeck_oracle_engine_contracts::model::{PlayableCardInput, compile_playable_card};
use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{
        CardPartRole, CardStructureKind, OracleCardFace, OracleCardParseRequest,
        parse_oracle_card_native,
    },
};

fn input(layout: &str, types: &[&str], texts: &[&str]) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Shared Structure".to_string(),
        type_line: types.join(" // "),
        mana_cost: None,
        oracle_text: if types.is_empty() {
            Some("Draw a card.".to_string())
        } else {
            None
        },
        layout: Some(layout.to_string()),
        faces: types
            .iter()
            .zip(texts)
            .enumerate()
            .map(|(index, (type_line, text))| OracleCardFace {
                id: format!("part-{index}"),
                name: format!("Part {index}"),
                type_line: type_line.to_string(),
                mana_cost: Some("{1}".to_string()),
                oracle_text: text.to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            })
            .collect(),
    }
}

fn compile(
    oracle: OracleCardParseRequest,
) -> Result<deepdeck_oracle_engine_contracts::model::PlayableCardCompilation, String> {
    compile_playable_card(PlayableCardInput {
        id: "structure-contract".to_string(),
        face_id: oracle.faces.first().map(|face| face.id.clone()),
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: None,
        toughness: None,
        oracle,
    })
}

#[test]
fn face_factory_routes_shared_primitive_rules_into_the_existing_runtime_adapters() {
    for (layout, types, texts, kind) in [
        (
            "split",
            ["Instant", "Sorcery"],
            ["Draw a card.", "You gain 2 life."],
            CardStructureKind::Split,
        ),
        (
            "modal_dfc",
            ["Instant", "Land"],
            ["Draw a card.", "{T}: Add {G}."],
            CardStructureKind::ModalDoubleFaced,
        ),
        (
            "transform",
            ["Creature", "Creature"],
            ["Flying", "Trample"],
            CardStructureKind::Transform,
        ),
        (
            "prepare",
            ["Creature", "Instant"],
            [
                "This creature enters prepared.",
                "Target player draws three cards.",
            ],
            CardStructureKind::Preparation,
        ),
    ] {
        let request = input(layout, &types, &texts);
        let parsed = parse_oracle_card_native(request.clone());
        assert_eq!(
            parsed.status, "canonical",
            "{layout}: {:?}",
            parsed.diagnostics
        );
        assert_eq!(parsed.card_structure.as_ref().unwrap().kind, kind);
        let compiled = compile(request).expect("supported topology reaches its existing adapter");
        assert_eq!(compiled.engine_status, "executable", "{layout}");
        assert!(!compiled.card.rules.is_empty(), "{layout}");
        assert!(
            compiled.card.rules.iter().all(rule_is_executable),
            "{layout}"
        );
        if kind == CardStructureKind::Preparation {
            let plan = compiled.parser_result.card_structure.as_ref().unwrap();
            assert_eq!(plan.parts[1].role, CardPartRole::PreparedSpell);
            let linked = compiled
                .card
                .rules
                .iter()
                .find(|rule| rule["kind"] == "prepareSpell")
                .unwrap();
            assert_eq!(linked["spell"]["id"], "part-1");
            assert_eq!(
                linked["spell"]["rules"][0]["effects"][0]["kind"],
                "drawCards"
            );
        }
    }
}

#[test]
fn unknown_or_malformed_structures_cannot_compile_to_an_empty_executable_card() {
    for (request, code) in [
        (
            input("future_layout", &[], &[]),
            "card_face_factory_missing",
        ),
        (
            input("transform", &["Creature"], &["Flying"]),
            "card_face_structure_invalid",
        ),
    ] {
        let failure =
            compile(request).expect_err("preflight failure must reach the compiler caller");
        assert!(failure.contains(code), "{failure}");
    }
}
