//! The effect-subject policy: an explicit subject followed by a clause.
//! `you VERB` is the controller's clause, read by the same action readers as
//! the imperative; verbs no reader takes fail at `clause.verb.unsupported`.
use serde_json::Value;

use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn parse(type_line: &str, oracle: &str) -> (String, Vec<Value>, Vec<String>) {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card_native(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    let states = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.iter())
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect();
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    (parsed.status, rules, states)
}

fn effects(type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
        .iter()
        .flat_map(|rule| rule["effects"].as_array().cloned().unwrap_or_default())
        .collect()
}

const CREATURE: &str = "Creature \u{2014} Elf";
const SORCERY: &str = "Sorcery";

/// `you VERB` and the imperative `VERB` are one clause: the same IR.
#[test]
fn the_controller_subject_reads_the_imperative_clause() {
    // Real cards: Eivor, Wolf-Kissed (mill), Cruel Ultimatum (a coordinated
    // list continuing the subject). Unused recombinations: scry, surveil and
    // investigate after an explicit `you`.
    for (type_line, explicit, imperative) in [
        (
            CREATURE,
            "Whenever this creature deals combat damage to a player, you mill that many cards.",
            "Whenever this creature deals combat damage to a player, mill that many cards.",
        ),
        (
            SORCERY,
            "Each opponent sacrifices a creature. You return a creature card from your graveyard to your hand, draw three cards, and gain 5 life.",
            "Each opponent sacrifices a creature. Return a creature card from your graveyard to your hand, draw three cards, and gain 5 life.",
        ),
        (
            CREATURE,
            "When this creature enters, you scry 2.",
            "When this creature enters, scry 2.",
        ),
        (
            CREATURE,
            "When this creature enters, you surveil 1, then you investigate.",
            "When this creature enters, surveil 1, then investigate.",
        ),
    ] {
        assert_eq!(
            effects(type_line, explicit),
            effects(type_line, imperative),
            "{explicit}"
        );
    }
}

/// An explicit `you` replaces the subject the previous clause would lend a
/// subjectless verb: the controller mills, not the target player.
#[test]
fn the_controller_subject_is_not_the_inherited_player() {
    let effects = effects(
        SORCERY,
        "Target player draws two cards, then you mill two cards.",
    );
    let mill = effects
        .iter()
        .find(|effect| effect["kind"] == "millCards" || effect["kind"] == "mill")
        .unwrap_or_else(|| panic!("a mill effect: {effects:#?}"));
    assert_ne!(mill["player"]["kind"], "targetPlayer", "{mill:#?}");
}

/// Near misses: a verb no action reader takes names its lemma and the
/// subject; modal continuations keep their own frames.
#[test]
fn an_unread_verb_of_the_controller_fails_at_the_clause() {
    let (status, _, states) = parse(
        CREATURE,
        "When this creature enters, you get an emblem with \"Creatures you control get +1/+1.\"",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "clause.verb.unsupported"),
        "{states:?}"
    );
    let (_, _, states) = parse(CREATURE, "When this creature enters, you may draw a card.");
    assert!(
        !states
            .iter()
            .any(|state| state == "clause.verb.unsupported"),
        "{states:?}"
    );
}

fn player_counters(type_line: &str, oracle: &str) -> Vec<Value> {
    fn collect(effect: &Value, found: &mut Vec<Value>) {
        if effect["kind"] == "addPlayerCounters" {
            found.push(effect.clone());
        }
        for child in effect["effects"].as_array().into_iter().flatten() {
            collect(child, found);
        }
    }
    let mut found = Vec::new();
    for effect in effects(type_line, oracle) {
        collect(&effect, &mut found);
    }
    found
}

/// `get` with a player-counters frame (CR 122.1): every player subject the
/// clause reads, one lowering.
#[test]
fn player_subjects_get_counters() {
    use serde_json::json;
    // Real cards: Pit Scorpion (event player), Pistus Strike (its
    // controller), Aethertorch Renegade (energy symbols and reminder).
    // Unused recombinations: a quantified population with a counter name
    // and a counted experience counter for the controller.
    for (type_line, oracle, player, counter, count) in [
        (
            "Creature \u{2014} Scorpion",
            "Whenever this creature deals damage to a player, that player gets a poison counter.",
            json!({"kind":"triggeringPlayer"}),
            "poison",
            1,
        ),
        (
            "Instant",
            "Destroy target creature with flying. Its controller gets a poison counter.",
            json!("controllerOf"),
            "poison",
            1,
        ),
        (
            CREATURE,
            "When this creature enters, you get {E}{E}{E}{E} (four energy counters).",
            json!({"kind":"controllerOf","object":{"kind":"self"}}),
            "energy",
            4,
        ),
        (
            CREATURE,
            "When this creature enters, each opponent gets two rad counters.",
            json!(null),
            "rad",
            2,
        ),
        (
            CREATURE,
            "When this creature enters, you get an experience counter.",
            json!({"kind":"controllerOf","object":{"kind":"self"}}),
            "experience",
            1,
        ),
    ] {
        let effects = player_counters(type_line, oracle);
        assert_eq!(effects.len(), 1, "{oracle}: {effects:#?}");
        let effect = &effects[0];
        assert_eq!(effect["counter"], counter, "{oracle}");
        assert_eq!(effect["count"]["value"], count, "{oracle}: {effect:#}");
        match player {
            Value::Null => {}
            Value::String(kind) => assert_eq!(effect["player"]["kind"], kind, "{oracle}"),
            player => assert_eq!(effect["player"], player, "{oracle}"),
        }
    }
}

/// Near misses: a reminder that miscounts the symbols fails; counters put
/// `on` an object stay with the counter-placement frame.
#[test]
fn player_counters_near_misses() {
    let (status, _, states) = parse(
        CREATURE,
        "When this creature enters, you get {E}{E} (three energy counters).",
    );
    assert_ne!(status, "canonical");
    assert!(
        states.iter().any(
            |state| state.starts_with("player-action.counters") || state.starts_with("numeric")
        ),
        "{states:?}"
    );
    assert!(
        player_counters(
            CREATURE,
            "When this creature enters, put a +1/+1 counter on target creature.",
        )
        .is_empty()
    );
}

/// `they` names the player `that player` would name, with the plural verb:
/// the same IR. A creature-population damage trigger records the damaged
/// player (CR 510.2), so its `that player` is the event's player.
#[test]
fn they_and_that_player_are_one_player_reference() {
    // Real cards: Sheoldred, the Apocalypse (draw event, life), Larceny
    // (population combat damage, discard). Unused recombinations: `they`
    // after a declared target player, `they` with discard.
    for (type_line, pronoun, phrase) in [
        (
            CREATURE,
            "Whenever an opponent draws a card, they lose 2 life.",
            "Whenever an opponent draws a card, that player loses 2 life.",
        ),
        (
            SORCERY,
            "Target player draws a card. They lose 1 life.",
            "Target player draws a card. That player loses 1 life.",
        ),
        (
            "Enchantment",
            "Whenever a creature you control deals combat damage to a player, they discard a card.",
            "Whenever a creature you control deals combat damage to a player, that player discards a card.",
        ),
    ] {
        assert_eq!(
            effects(type_line, pronoun),
            effects(type_line, phrase),
            "{pronoun}"
        );
    }
    let discard = effects(
        "Enchantment",
        "Whenever a creature you control deals combat damage to a player, that player discards a card.",
    );
    assert!(
        discard
            .iter()
            .any(|effect| effect["player"]["kind"] == "triggeringPlayer"),
        "{discard:#?}"
    );
}

/// Near misses: `they` without a player binding is not a player subject, and
/// a plural object antecedent keeps its own readers.
#[test]
fn they_without_a_player_is_not_claimed() {
    let (status, _, _) = parse(CREATURE, "When this creature enters, they lose 2 life.");
    assert_ne!(status, "canonical");
    let (status, _, _) = parse(CREATURE, "When this creature enters, they discard a card.");
    assert_ne!(status, "canonical");
}

/// `For each COUNTABLE, ACTION.` is the trailing multiplier written first:
/// the same IR as `ACTION for each COUNTABLE`.
#[test]
fn a_preposed_for_each_scales_its_action() {
    // Real card shape: Tribune of Rot / Moogles' Valor (count, then the one
    // action). Unused recombinations: draw and damage scaled by a count.
    for (preposed, trailing) in [
        (
            "For each creature you control, create a 1/1 white Soldier creature token.",
            "Create a 1/1 white Soldier creature token for each creature you control.",
        ),
        (
            "For each artifact you control, draw a card.",
            "Draw a card for each artifact you control.",
        ),
        (
            "For each creature you control, this spell deals 1 damage to any target.",
            "This spell deals 1 damage to any target for each creature you control.",
        ),
    ] {
        assert_eq!(
            effects(SORCERY, preposed),
            effects(SORCERY, trailing),
            "{preposed}"
        );
    }
}

/// Near miss: the introducer scopes one action ending its sentence. A
/// player population is not a countable: it opens the distributive player
/// frame (a loop over the players, not a count).
#[test]
fn a_preposed_for_each_scopes_one_counted_action() {
    let (status, _, states) = parse(
        SORCERY,
        "For each creature you control, draw a card and gain 1 life.",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "sequence.for-each.preposed-action"),
        "{states:?}"
    );
    let distributive = effects(
        SORCERY,
        "For each opponent, create a 1/1 white Soldier creature token.",
    );
    assert_eq!(distributive.len(), 1, "{distributive:#?}");
    assert_eq!(
        distributive[0]["kind"], "forEachPlayer",
        "{distributive:#?}"
    );
}

fn untap_restrictions(type_line: &str, oracle: &str) -> Vec<Value> {
    effects(type_line, oracle)
        .into_iter()
        .filter(|effect| effect["kind"] == "installUntapRestriction")
        .collect()
}

/// The negated `untap` clause of a permanent subject (CR 502.3): the
/// subject is the shared permanent reference, the window is the controller's
/// next untap step or the source's control duration.
#[test]
fn a_permanent_subject_does_not_untap() {
    // Real cards: Frost Lynx (`that creature`, next step), Chill of the Grave
    // (`it`), Tidebinder Mage (while the source is controlled). Unused
    // recombination: the restriction after a stat change.
    for (type_line, oracle, duration) in [
        (
            CREATURE,
            "When this creature enters, tap target creature an opponent controls. That creature doesn't untap during its controller's next untap step.",
            "nextUntapStep",
        ),
        (
            "Instant",
            "Tap target creature. It doesn't untap during its controller's next untap step.",
            "nextUntapStep",
        ),
        (
            CREATURE,
            "When this creature enters, tap target red or green creature an opponent controls. That creature doesn't untap during its controller's untap step for as long as you control this creature.",
            "whileSourceControlled",
        ),
        (
            "Instant",
            "Target creature gets -2/-0 until end of turn. It doesn't untap during its controller's next untap step.",
            "nextUntapStep",
        ),
        // `your`: one player's next untap step (CR 502.3).
        (
            "Instant",
            "Tap target creature. It doesn't untap during your next untap step.",
            "nextUntapStep",
        ),
    ] {
        let restrictions = untap_restrictions(type_line, oracle);
        assert_eq!(restrictions.len(), 1, "{oracle}");
        assert_eq!(restrictions[0]["duration"]["kind"], duration, "{oracle}");
        assert_eq!(
            restrictions[0]["permanent"]["kind"], "chosenTarget",
            "{oracle}: {restrictions:#?}"
        );
    }
}

/// Near misses: the negation agrees with a declared subject, and the untap
/// step names its owner (`its controller's`, `your`, `that player's`).
#[test]
fn a_permanent_subject_does_not_untap_near_misses() {
    for oracle in [
        "Tap target creature. That creature don't untap during its controller's next untap step.",
        "Tap target creature. It doesn't untap during the next untap step.",
    ] {
        let (status, _, _) = parse("Instant", oracle);
        assert_ne!(status, "canonical", "{oracle}");
    }
}

/// CR 400.7: the effect that put one card onto the battlefield finds the
/// permanent it became; `it` and `that creature` name it.
#[test]
fn the_card_a_move_put_onto_the_battlefield_is_the_antecedent() {
    // Real card: Bond of Revival (`it`). Sibling: the nominal anaphor gives
    // the same IR. Unused recombination: a counter on the returned card.
    let pronoun = effects(
        SORCERY,
        "Return target creature card from your graveyard to the battlefield. It gains haste until end of turn.",
    );
    let nominal = effects(
        SORCERY,
        "Return target creature card from your graveyard to the battlefield. That creature gains haste until end of turn.",
    );
    assert_eq!(pronoun, nominal);
    let counters = effects(
        SORCERY,
        "Return target creature card from your graveyard to the battlefield. Put a +1/+1 counter on it.",
    );
    let moved = counters
        .iter()
        .find(|effect| effect["kind"] == "moveCards")
        .expect("the move");
    let binding = moved["bind"].clone();
    assert!(binding.is_string(), "{moved:#}");
    let placement = counters
        .iter()
        .find(|effect| effect["kind"] == "putCounters")
        .expect("the counters");
    assert_eq!(placement["permanent"]["binding"], binding, "{placement:#}");
}

/// Near misses: the nominal must follow from the moved card's predicate, and
/// a move of several cards is no singular antecedent.
#[test]
fn the_moved_card_antecedent_near_misses() {
    for oracle in [
        "Return target creature card from your graveyard to the battlefield. That land gains haste until end of turn.",
        "Return up to two target creature cards from your graveyard to the battlefield. It gains haste until end of turn.",
    ] {
        let (status, _, _) = parse(SORCERY, oracle);
        assert_ne!(status, "canonical", "{oracle}");
    }
}

/// CR 603.12: `When you do,` after any sentence of a spell is the reflexive
/// frame of that sentence's instruction; the spell body is an effect program
/// exactly as a triggered ability's body is.
#[test]
fn a_spell_sentence_opens_a_reflexive_frame() {
    // Real cards: Faebloom Trick (first sentence), Lithobraking (`Then you
    // may` permission after a first step). Unused recombination: the spell
    // and the triggered ability give the same program.
    let spell = effects(
        SORCERY,
        "Sacrifice a creature. When you do, target creature gets -2/-2 until end of turn.",
    );
    let triggered = effects(
        CREATURE,
        "When this creature enters, sacrifice a creature. When you do, target creature gets -2/-2 until end of turn.",
    );
    assert_eq!(spell, triggered);
    assert_eq!(spell[0]["kind"], "performAction", "{spell:#?}");
    let faebloom = effects(
        "Instant",
        "Create two 1/1 blue Faerie creature tokens with flying. When you do, tap target creature an opponent controls.",
    );
    assert_eq!(faebloom[0]["kind"], "performAction", "{faebloom:#?}");
    let later = effects(
        SORCERY,
        "Mill two cards. Then you may sacrifice a creature. When you do, draw a card.",
    );
    assert_eq!(later[0]["kind"], "mill", "{later:#?}");
    assert_eq!(later.len(), 2, "{later:#?}");
}

/// Near miss: an instruction the runtime cannot measure as one performance
/// fails as the explicit measured-performance engine gap (scry looks at and
/// orders cards; CR 701.22) instead of being approximated.
#[test]
fn a_reflexive_frame_needs_one_measured_instruction() {
    let (status, _, states) = parse(SORCERY, "Scry 1. When you do, draw a card.");
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "engine.gap.measured-performance"),
        "{states:?}"
    );
}

/// `For each opponent, BODY` is the distributive player frame (CR 608.2c),
/// never a count. A body without targets loops over the players; an exact
/// target declared per player (CR 601.2c, Blatant Thievery) has no
/// per-player declaration yet and fails explicitly in the frame lowering.
#[test]
fn a_player_population_is_a_distributive_frame() {
    // Real cards: Blatant Thievery (per-player targets), Soul of Eternity's
    // shape (no target).
    let (status, _, states) = parse(
        SORCERY,
        "For each opponent, gain control of target permanent that player controls.",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "lower.for-each-player.per-player-targets"),
        "{states:?}"
    );
    let looped = effects(
        SORCERY,
        "For each player, create a 1/1 white Soldier creature token.",
    );
    assert_eq!(looped.len(), 1, "{looped:#?}");
    assert_eq!(looped[0]["kind"], "forEachPlayer", "{looped:#?}");
}

/// The performer slot (CR 608.2c): another player's clause is the ordinary
/// instruction with that player as its performer; a population performs it
/// in a player loop.
#[test]
fn another_player_performs_the_ordinary_instruction() {
    use serde_json::json;
    // Real cards: Goblin Spymaster (event player creates), Ulamog, the
    // Ceaseless Hunger's sibling (its owner shuffles), Exhume (population).
    let created = effects(
        "Enchantment",
        "At the beginning of each opponent's end step, that player creates a 1/1 red Goblin creature token.",
    );
    assert_eq!(created.len(), 1, "{created:#?}");
    assert_eq!(created[0]["performer"], json!({"kind":"triggeringPlayer"}));
    let imperative = effects(
        "Enchantment",
        "At the beginning of each opponent's end step, create a 1/1 red Goblin creature token.",
    );
    let mut without_performer = created[0].clone();
    without_performer
        .as_object_mut()
        .expect("an effect object")
        .remove("performer");
    assert_eq!(without_performer, imperative[0], "the same instruction");
    let exhumed = effects(
        SORCERY,
        "Each player puts a creature card from their graveyard onto the battlefield.",
    );
    assert_eq!(exhumed[0]["kind"], "forEachPlayer", "{exhumed:#?}");
    let bind = exhumed[0]["bind"].clone();
    assert!(
        exhumed[0]["effects"]
            .as_array()
            .expect("a loop body")
            .iter()
            .all(|effect| effect["performer"] == json!({"kind":"boundValue","id":bind})),
        "{exhumed:#?}"
    );
    // Unused recombination: an opponent population creating tokens.
    let tokens = effects(SORCERY, "Each opponent creates a Treasure token.");
    assert_eq!(tokens[0]["kind"], "forEachPlayer", "{tokens:#?}");
}

/// Near misses: the verb must agree with the performer, and a possessive
/// performer needs its object.
#[test]
fn performer_near_misses() {
    for oracle in [
        "Each player create a 1/1 green Saproling creature token.",
        "Its owner creates a 1/1 white Spirit creature token with flying.",
    ] {
        let (status, _, _) = parse(SORCERY, oracle);
        assert_ne!(status, "canonical", "{oracle}");
    }
}

/// A subjectless verb of the same sentence continues the performer of the
/// previous action (CR 608.2c), whichever reader read that action.
#[test]
fn a_later_verb_of_the_sentence_continues_the_performer() {
    // Real card shape: Mirko Vosk / Destroy the Evidence (`its controller
    // reveals ..., then puts ...`). Unused recombination: a population that
    // mills, then creates.
    let effects = effects(
        SORCERY,
        "Each opponent mills two cards, then creates a Treasure token.",
    );
    let created = effects
        .iter()
        .find(|effect| {
            effect["kind"] == "forEachPlayer"
                && effect["effects"][0]["kind"] != "mill"
                && effect["effects"][0]["performer"].is_object()
        })
        .unwrap_or_else(|| panic!("a performed creation: {effects:#?}"));
    assert_eq!(created["players"]["kind"], "opponentsOf", "{created:#}");
    // Near miss: the performer does not cross a sentence boundary.
    let (status, _, _) = parse(
        SORCERY,
        "Each opponent mills two cards. Creates a Treasure token.",
    );
    assert_ne!(status, "canonical");
}

/// CR 611.2a: a continuous-effect clause that states no duration lasts
/// indefinitely; only primitives with an indefinite form accept it.
#[test]
fn a_clause_without_a_duration_is_indefinite() {
    // Real card: Fated Return's sibling (`That creature gains haste.` after a
    // return). Unused recombination: a targeted flying grant.
    for oracle in [
        "Return target creature card from your graveyard to the battlefield. That creature gains haste.",
        "Target creature gains flying.",
    ] {
        let grants = effects(SORCERY, oracle)
            .into_iter()
            .filter(|effect| effect["kind"] == "grantKeyword")
            .collect::<Vec<_>>();
        assert_eq!(grants.len(), 1, "{oracle}");
        assert_eq!(grants[0]["duration"]["kind"], "permanent", "{oracle}");
    }
    // Near miss: a stat change has no indefinite primitive yet.
    let (status, _, _) = parse(SORCERY, "Target creature gets +2/+2.");
    assert_ne!(status, "canonical");
}

/// `exile|sacrifice it` on the permanent a move put onto the battlefield is
/// the bound permanent, and a delayed timing captures it (CR 603.7c).
#[test]
fn a_delayed_action_on_the_returned_permanent() {
    // Real card: Goryo's Vengeance. Unused recombination: a sacrifice of the
    // returned card at the end step.
    for (oracle, kind) in [
        (
            "Return target legendary creature card from your graveyard to the battlefield. That creature gains haste. Exile it at the beginning of the next end step.",
            "exilePermanent",
        ),
        (
            "Return target creature card from your graveyard to the battlefield. Sacrifice it at the beginning of the next end step.",
            "sacrificePermanent",
        ),
    ] {
        let effects = effects("Instant", oracle);
        let delayed = effects
            .iter()
            .find(|effect| effect["kind"] == "installDelayedStepTrigger")
            .unwrap_or_else(|| panic!("a delayed trigger: {effects:#?}"));
        assert_eq!(delayed["effects"][0]["kind"], kind, "{delayed:#}");
        assert!(
            delayed["captureBindings"]
                .as_array()
                .is_some_and(|bindings| !bindings.is_empty()),
            "{delayed:#}"
        );
    }
    // Near miss: a card returned to the hand is no permanent antecedent.
    let (status, _, _) = parse(
        "Instant",
        "Return target creature card from your graveyard to your hand. Sacrifice it at the beginning of the next end step.",
    );
    assert_ne!(status, "canonical");
}

/// The card an optional deployment put onto the battlefield is the
/// antecedent of the next sentences; the move binds it only when read.
#[test]
fn the_deployed_card_is_the_antecedent_across_sentences() {
    // Real cards: Sneak Attack, Through the Breach (haste, then a delayed
    // sacrifice). Unused recombination: a counter on the deployed card.
    let sneak = effects(
        "Enchantment",
        "{R}: You may put a creature card from your hand onto the battlefield. That creature gains haste. Sacrifice the creature at the beginning of the next end step.",
    );
    let movement = sneak
        .iter()
        .find(|effect| effect["kind"] == "moveCards")
        .unwrap_or_else(|| panic!("the deployment: {sneak:#?}"));
    assert!(movement["bind"].is_string(), "{movement:#}");
    assert!(movement.get("__deployedCard").is_none(), "{movement:#}");
    assert!(
        sneak
            .iter()
            .any(|effect| effect["kind"] == "installDelayedStepTrigger"),
        "{sneak:#?}"
    );
    let counter = effects(
        SORCERY,
        "You may put a creature card from your hand onto the battlefield. Put a +1/+1 counter on it.",
    );
    assert!(
        counter.iter().any(|effect| effect["kind"] == "putCounters"
            && effect["permanent"]["kind"] == "singleBoundObject"),
        "{counter:#?}"
    );
    // Without a later reader the move binds nothing (unchanged IR).
    let alone = effects(
        SORCERY,
        "You may put a creature card from your hand onto the battlefield.",
    );
    assert!(
        alone
            .iter()
            .all(|effect| effect.get("bind").is_none() && effect.get("__deployedCard").is_none()),
        "{alone:#?}"
    );
}

/// A written `you` inside another player's clause names the ability's
/// controller; the performer's instruction would read it as the performer.
/// That reading is ambiguous and fails explicitly.
#[test]
fn a_controller_reference_inside_a_performed_clause_fails_explicitly() {
    for oracle in [
        "Each opponent returns a creature you control to its owner's hand.",
        "Each opponent puts a creature card from your graveyard onto the battlefield.",
    ] {
        let (status, _, states) = parse(SORCERY, oracle);
        assert_ne!(status, "canonical", "{oracle}");
        assert!(
            states
                .iter()
                .any(|state| state == "clause.performer.controller-reference"),
            "{oracle}: {states:?}"
        );
    }
    // The performer's own pronouns are not controller references.
    let (status, _, _) = parse(
        SORCERY,
        "Each player puts a creature card from their graveyard onto the battlefield.",
    );
    assert_eq!(status, "canonical");
}

/// A quoted ability is its own scope: its `you` is not a controller
/// reference of the performed clause (Goblin Spymaster).
#[test]
fn a_quoted_ability_inside_a_performed_clause_keeps_its_own_you() {
    let (status, _, states) = parse(
        "Enchantment",
        "At the beginning of each opponent's end step, that player creates a 1/1 red Goblin creature token with \"Creatures you control attack each combat if able.\"",
    );
    assert_eq!(status, "canonical", "{states:?}");
}

/// The guard reads only the performed action's tokens: a later `you`
/// clause of the same sentence is the controller's own action.
#[test]
fn a_later_controller_clause_is_not_inside_the_performed_action() {
    let (status, _, states) = parse(
        SORCERY,
        "Each opponent creates a Treasure token, then you draw a card.",
    );
    assert_eq!(status, "canonical", "{states:?}");
}

/// One reader for zone actions on a bound permanent; target antecedents.
#[test]
fn zone_actions_on_a_target_antecedent() {
    // Non-delayed: the declared target is the permanent acted on.
    for (oracle, kind) in [
        ("Tap target creature. Exile it.", "exilePermanent"),
        ("Tap target creature. Sacrifice it.", "sacrificePermanent"),
        (
            "Tap target creature. Sacrifice the creature.",
            "sacrificePermanent",
        ),
    ] {
        let effects = effects("Instant", oracle);
        let action = effects
            .iter()
            .find(|effect| effect["kind"] == kind)
            .unwrap_or_else(|| panic!("{oracle}: {effects:#?}"));
        assert_eq!(action["permanent"]["kind"], "chosenTarget", "{oracle}");
    }
    // A delayed action on a declared target captures that target when the
    // delayed trigger is created (CR 603.7c) and reads the capture.
    let effects = effects(
        "Instant",
        "Tap target creature. Exile it at the beginning of the next end step.",
    );
    let delayed = effects
        .iter()
        .find(|effect| effect["kind"] == "installDelayedStepTrigger")
        .unwrap_or_else(|| panic!("{effects:#?}"));
    assert_eq!(
        delayed["captureReferences"][0]["reference"]["kind"],
        "chosenTarget"
    );
    assert_eq!(delayed["effects"][0]["kind"], "exilePermanent");
    assert_eq!(delayed["effects"][0]["permanent"]["kind"], "boundValue");
    // The definite description follows the antecedent's selector.
    let (status, _, _) = parse("Instant", "Tap target creature. Sacrifice the land.");
    assert_ne!(status, "canonical");
}

/// Indefinite durations: a combat restriction without a duration, the
/// static near miss, and the refusal of an indefinite grant to a spell.
#[test]
fn indefinite_restrictions_and_the_spell_grant_refusal() {
    let restriction = effects("Instant", "Target creature can't block.");
    assert!(!restriction.is_empty(), "{restriction:#?}");
    let (status, rules, _) = parse("Creature \u{2014} Wall", "This creature can't block.");
    assert_eq!(status, "canonical");
    assert_eq!(rules[0]["kind"], "staticAbility", "{rules:#?}");
    let (status, _, states) = parse(SORCERY, "Draw a card. This spell gains haste.");
    assert_ne!(status, "canonical", "{states:?}");
}

/// Near misses that stay non-canonical: a countered spell is no battlefield
/// antecedent, a definite description with its own qualifier is no anaphor,
/// and a delayed or conditional `it` without a proven permanent stays refused.
#[test]
fn zone_action_near_misses_stay_non_canonical() {
    for (type_line, oracle) in [
        ("Instant", "Counter target spell. Exile it."),
        (
            "Creature \u{2014} Goblin",
            "{T}: Choose target creature an opponent controls. Destroy the creature you chose.",
        ),
        (
            "Enchantment",
            "At the beginning of your upkeep, destroy the creature with the least power.",
        ),
        (
            "Enchantment",
            "When this enchantment enters, return target creature card from your graveyard to the battlefield. When the creature put onto the battlefield with this enchantment dies, exile it and return this enchantment to its owner's hand.",
        ),
        (
            SORCERY,
            "Reorder your graveyard at random. An opponent chooses a card at random in your graveyard. If it's a creature card, put it onto the battlefield. Otherwise, exile it.",
        ),
    ] {
        let (status, _, states) = parse(type_line, oracle);
        assert_ne!(status, "canonical", "{oracle}: {states:?}");
    }
}

/// CR 102.3: `each other player` is every player but the controller,
/// teammates included, never the opponents set.
#[test]
fn each_other_player_is_the_controllers_complement() {
    let effects = effects(SORCERY, "Each other player creates a Treasure token.");
    assert_eq!(effects[0]["kind"], "forEachPlayer", "{effects:#?}");
    assert_eq!(
        effects[0]["players"]["kind"], "playersExcept",
        "{effects:#?}"
    );
}

/// An indefinite grant needs a battlefield object (CR 611.2a, 400.7,
/// 603.10a), decided on the typed operand: plural declared targets take it;
/// an event's permanent that may be last known information does not.
#[test]
fn an_indefinite_grant_reads_the_operand_zone() {
    let grants = effects(SORCERY, "Up to two target creatures gain flying.");
    assert!(
        grants
            .iter()
            .all(|effect| effect["duration"]["kind"] == "permanent"),
        "{grants:#?}"
    );
    let (status, _, _) = parse("Enchantment", "Whenever a creature dies, it gains flying.");
    assert_ne!(status, "canonical");
}
