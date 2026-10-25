#[path = "../src/local_deck_flags.rs"]
mod flags;
use serde_json::json;

#[test]
fn section_only_sideboard_and_companion_stay_out_of_the_main_deck() {
    for section in ["sideboard", "companion", " SideBoard "] {
        let card = json!({"section":section});
        assert!(flags::is_sideboard(&card, &card));
    }
    for section in ["main", "commander", "considering"] {
        let card = json!({"section":section});
        assert!(!flags::is_sideboard(&card, &card));
    }
}

#[test]
fn boolean_and_selected_printing_storage_remain_supported() {
    assert!(flags::is_sideboard(
        &json!({"isSideboard":true}),
        &json!({})
    ));
    assert!(flags::is_sideboard(
        &json!({"isSideboard":false}),
        &json!({"isSideboard":true})
    ));
    assert!(flags::is_sideboard(
        &json!({}),
        &json!({"isCompanion":true})
    ));
}

#[test]
fn tokens_markers_and_other_auxiliaries_are_never_deck_cards() {
    for line in [
        "Token",
        "Token Creature — Orc Army",
        "Emblem",
        "Dungeon",
        "Card",
        "Counter",
        "Marker",
        "Checklist",
        "Helper",
        "Sticker",
        "Art Card",
    ] {
        let card = json!({"typeLine":line});
        assert!(flags::is_auxiliary(&card, &card), "{line}");
    }
    assert!(flags::is_auxiliary(
        &json!({}),
        &json!({"layout":"double_faced_token"})
    ));
    assert!(flags::is_auxiliary(&json!({"isToken":true}), &json!({})));
}

#[test]
fn real_companion_and_incomplete_cards_are_not_hidden_as_auxiliaries() {
    for card in [
        json!({"name":"Yorion, Sky Nomad","typeLine":"Legendary Creature — Bird Serpent"}),
        json!({}),
        json!({"typeLine":"Artifact Creature — Construct"}),
    ] {
        assert!(!flags::is_auxiliary(&card, &card));
    }
}

#[test]
fn commander_export_with_fifteen_reserve_cards_has_one_hundred_main_cards() {
    let mut cards = vec![
        json!({"section":"main","quantity":99}),
        json!({"section":"commander","quantity":1}),
        json!({"section":"sideboard","quantity":15}),
    ];
    cards.push(json!({"section":"main","typeLine":"Card","quantity":1}));
    let count: u64 = cards
        .iter()
        .filter(|c| !flags::is_sideboard(c, c) && !flags::is_auxiliary(c, c))
        .map(|c| c["quantity"].as_u64().unwrap())
        .sum();
    assert_eq!(count, 100);
}
