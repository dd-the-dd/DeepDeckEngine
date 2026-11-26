//! `the top N cards of your library` is one card-collection operand. `reveal`,
//! `exile`, `put` and the permission verb read the same bound set, so a new
//! combination of these concepts needs no new production code.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::Value;

fn parse(type_line: &str, cost: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn effects_of(parsed: &OracleCardParseResult, ability: usize) -> Vec<Value> {
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    parsed.abilities[ability].rule.as_ref().expect("a rule")["effects"]
        .as_array()
        .expect("effects")
        .clone()
}

fn kinds(effects: &[Value]) -> Vec<&str> {
    effects
        .iter()
        .map(|effect| effect["kind"].as_str().expect("kind"))
        .collect()
}

fn failing_state(parsed: &OracleCardParseResult) -> Option<String> {
    let ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.status != "canonical")?;
    ability
        .diagnostics
        .iter()
        .find_map(|diagnostic| diagnostic.state.clone())
}

fn assert_not_canonical(text: &str) {
    let parsed = parse("Sorcery", "{1}{R}", text);
    assert_ne!(parsed.status, "canonical", "{text}");
    assert!(failing_state(&parsed).is_some(), "{text}");
}

#[test]
fn impulse_draw_binds_the_top_card_before_exiling_and_permitting_it() {
    let parsed = parse(
        "Creature \u{2014} Human Wizard",
        "{1}{R}",
        "Prowess\nWhen this creature enters, exile the top card of your library. Until end of turn, you may play that card.",
    );
    let effects = effects_of(&parsed, 1);
    assert_eq!(kinds(&effects), ["bind", "moveCards", "grantPermission"]);
    assert_eq!(effects[0]["value"]["kind"], "libraryTopCards");
    assert_eq!(effects[1]["to"]["kind"], "exile");
    assert_eq!(effects[2]["action"]["kind"], "play");
    assert_eq!(effects[2]["action"]["card"]["binding"], effects[0]["id"]);
    assert_eq!(effects[2]["duration"]["kind"], "untilEndOfCurrentTurn");
}

#[test]
fn activated_impulse_draw_reads_the_end_of_the_next_turn() {
    let parsed = parse(
        "Land",
        "",
        "{T}: Add {R}.\n{3}{R}, {T}: Exile the top card of your library. Until the end of your next turn, you may play that card.",
    );
    let effects = effects_of(&parsed, 1);
    assert_eq!(kinds(&effects), ["bind", "moveCards", "grantPermission"]);
    assert_eq!(effects[2]["duration"]["kind"], "untilEndOfNextTurn");
}

#[test]
fn a_plural_window_trailing_or_preposed_shares_one_permission_reader() {
    for text in [
        "Exile the top three cards of your library. Until end of turn, you may play those cards.",
        "Exile the top three cards of your library. You may play those cards this turn.",
        "Exile the top three cards of your library. You may play those cards until the end of your next turn.",
        "Exile the top three cards of your library. Until the end of your next turn, you may play those cards.",
    ] {
        let effects = effects_of(&parse("Sorcery", "{1}{R}", text), 0);
        assert_eq!(effects[0]["value"]["count"]["value"], 3, "{text}");
        assert_eq!(
            effects[2]["action"]["card"]["kind"], "boundObjects",
            "{text}"
        );
    }
    // The same permission verb composes with a variable quantity and `cast`.
    let effects = effects_of(
        &parse(
            "Sorcery",
            "{X}{R}{R}",
            "Exile the top X cards of your library. You may cast those cards this turn.",
        ),
        0,
    );
    assert_eq!(effects[2]["action"]["kind"], "cast");
}

#[test]
fn reveal_and_put_all_matching_cards_read_one_revealed_set() {
    let effects = effects_of(
        &parse(
            "Sorcery",
            "{2}{G}",
            "Reveal the top three cards of your library. Put all creature cards revealed this way into your hand and the rest into your graveyard.",
        ),
        0,
    );
    assert_eq!(
        kinds(&effects),
        ["bind", "revealCards", "moveCards", "moveCards"]
    );
    assert_eq!(effects[2]["cards"]["kind"], "filterObjects");
    assert_eq!(effects[3]["cards"]["kind"], "setDifference");
    // The same selection reads a `from among them` observation unchanged.
    let looked = effects_of(
        &parse(
            "Sorcery",
            "{2}{G}",
            "Look at the top five cards of your library. Put all creature cards from among them into your hand and the rest on the bottom of your library in any order.",
        ),
        0,
    );
    assert_eq!(looked[1]["cards"]["kind"], "filterObjects");
}

#[test]
fn a_revealed_top_card_can_be_the_object_of_a_choice_and_a_move() {
    for text in [
        "Reveal the top card of your library and put that card into your hand.",
        "Reveal the top two cards of your library. Put one of them into your hand and the other into your graveyard.",
        "Reveal the top five cards of your library. You may put a creature or enchantment card from among them into your hand. Put the rest into your graveyard.",
        "Put the top two cards of your library into your graveyard.",
    ] {
        let parsed = parse("Sorcery", "{1}{G}", text);
        assert_eq!(
            parsed.status, "canonical",
            "{text}: {:#?}",
            parsed.abilities
        );
    }
}

#[test]
fn unbounded_permission_over_exiled_cards_lasts_while_they_remain_exiled() {
    // CR 611.2a: no stated window lasts for the game; CR 400.7 ends it when
    // the card leaves exile. Only cards this program exiled are admitted.
    let parsed = parse(
        "Sorcery",
        "{1}{R}",
        "Exile the top card of your library. You may play that card.",
    );
    let effects = effects_of(&parsed, 0);
    assert_eq!(kinds(&effects), ["bind", "moveCards", "grantPermission"]);
    assert_eq!(effects[2]["duration"]["kind"], "whileInExile");
    // Revealed (not exiled) cards stay in the library: with no stated
    // window the permission is used while the spell resolves (Djinn of
    // Wishes, CR 608.2g), from the caster's own observed library cards.
    let parsed = parse(
        "Sorcery",
        "{1}{R}",
        "Reveal the top card of your library. You may play that card.",
    );
    let effects = effects_of(&parsed, 0);
    assert_eq!(kinds(&effects), ["bind", "revealCards", "castAnyNumber"]);
    assert_eq!(effects[2]["sourceZone"], "library");
    assert_eq!(effects[2]["timing"]["kind"], "duringResolution");
    assert_eq!(effects[2]["playLands"], true);
    // Near miss: a singular `that card` over two revealed cards names no
    // single card to play.
    assert_not_canonical("Reveal the top two cards of your library. You may play that card.");
}

/// The library holder is the shared possessive: a target player's library
/// declares that player, and the permission reads the same bound card.
#[test]
fn a_target_players_library_top_is_the_same_operand() {
    let parsed = parse(
        "Sorcery",
        "{1}{R}",
        "Exile the top card of target player's library. You may play that card this turn.",
    );
    let effects = effects_of(&parsed, 0);
    assert_eq!(kinds(&effects), ["bind", "moveCards", "grantPermission"]);
    let rule = parsed.abilities[0].rule.as_ref().unwrap();
    let holder = &rule["declaration"]["decisions"][0];
    assert_eq!(holder["candidates"]["kind"], "players");
    assert_eq!(effects[0]["value"]["zone"]["player"]["id"], holder["id"]);
    assert_eq!(effects[2]["action"]["card"]["binding"], effects[0]["id"]);
}

#[test]
fn unsupported_neighbours_fail_explicitly() {
    // `until your next turn` is the same permission reader with a window that
    // ends when that turn begins (CR 611.2b); it is no longer a neighbour.
    let effects = effects_of(
        &parse(
            "Sorcery",
            "{1}{R}",
            "Exile the top card of your library. You may play that card until your next turn.",
        ),
        0,
    );
    assert_eq!(kinds(&effects), ["bind", "moveCards", "grantPermission"]);
    assert_eq!(effects[2]["duration"]["kind"], "untilNextTurn");
    // Another zone is not this operand: the top of a graveyard is the
    // graveyard-position operand (CR 404.2), not a library-top binding.
    let graveyard = parse(
        "Sorcery",
        "{1}{R}",
        "Exile the top card of your graveyard. You may play that card this turn.",
    );
    let effects = effects_of(&graveyard, 0);
    assert_eq!(effects[0]["value"]["kind"], "graveyardEndCards");
    // `revealed this way` must restate the verb that produced the collection.
    assert_not_canonical(
        "Exile the top three cards of your library. Put all cards revealed this way into your graveyard.",
    );
    // A sentence-level duration still belongs to duration-bearing effects only.
    assert_not_canonical("Until the end of your next turn, target creature gets +1/+1.");
}

#[test]
fn bounded_reveal_observes_every_revealed_card_and_names_the_matching_ones() {
    for (text, count, rest_kind) in [
        (
            "Reveal cards from the top of your library until you reveal a creature card. Put that card into your hand and the rest on the bottom of your library in a random order.",
            1,
            "setDifference",
        ),
        (
            "Reveal cards from the top of your library until you reveal two land cards. Put those cards into your hand and the rest into your graveyard.",
            2,
            "setDifference",
        ),
        (
            "Reveal cards from the top of your library until you reveal a land card. Put that card into your hand and all other cards revealed this way into your graveyard.",
            1,
            "setDifference",
        ),
    ] {
        let effects = effects_of(&parse("Sorcery", "{2}{G}", text), 0);
        assert_eq!(
            kinds(&effects),
            ["revealFromTopUntilCount", "moveCards", "moveCards"],
            "{text}"
        );
        assert_eq!(effects[0]["count"]["value"], count, "{text}");
        assert_eq!(effects[1]["cards"]["kind"], "filterObjects", "{text}");
        assert_eq!(effects[2]["cards"]["kind"], rest_kind, "{text}");
        assert_eq!(effects[2]["cards"]["left"]["binding"], effects[0]["bind"]);
    }
}

#[test]
fn bounded_reveal_rejects_what_the_runtime_cannot_yet_prove() {
    // The reveal fixes the identities, so a battlefield entry of the matching
    // card reads the same bounded observation as a hand destination.
    let effects = effects_of(
        &parse(
            "Sorcery",
            "{2}{G}",
            "Reveal cards from the top of your library until you reveal a creature card. Put that card onto the battlefield and the rest on the bottom of your library in a random order.",
        ),
        0,
    );
    assert_eq!(
        kinds(&effects),
        ["revealFromTopUntilCount", "moveCards", "moveCards"]
    );
    // Another library owner is a different (targeted) operand.
    assert_not_canonical(
        "Reveal cards from the top of target player's library until you reveal a land card. Put that card into your hand.",
    );
}
