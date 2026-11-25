//! Target lists, distributive subjects and permission scopes, proved from
//! the shared target-list child up to the complete card IR.
use super::super::ObjectQualifier;
use super::*;
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};
use serde_json::{Value, json};

fn cardinality(text: &str) -> MachineResult<(u32, u32)> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("target-list-range", &tokens);
    let list = recognize_target_list(&mut cursor)?;
    Ok((
        list.slots[0].cardinality.minimum,
        list.slots[0].cardinality.maximum,
    ))
}

#[test]
fn a_literal_range_is_one_bounded_target_cardinality() {
    assert_eq!(cardinality("one or two target creatures").unwrap(), (1, 2));
    assert_eq!(cardinality("two target creatures").unwrap(), (2, 2));
    // Near misses: a decreasing range and a variable upper bound have no
    // represented meaning and must not be read as `N target`.
    assert!(cardinality("two or one target creatures").is_err());
    assert!(cardinality("one or X target creatures").is_err());
}

fn card(name: &str, type_line: &str, text: &str) -> (String, Option<Value>) {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card_native(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    let rule = parsed
        .abilities
        .first()
        .and_then(|ability| ability.rule.clone());
    (parsed.status, rule)
}

fn canonical(name: &str, type_line: &str, text: &str) -> Value {
    let (status, rule) = card(name, type_line, text);
    assert_eq!(status, "canonical", "{text}");
    rule.expect("canonical rule")
}

fn unsupported(text: &str) {
    assert_ne!(card("Near Miss", "Sorcery", text).0, "canonical", "{text}");
}

#[test]
fn every_instance_of_target_is_its_own_declaration_in_one_collection() {
    let rule = canonical(
        "Plague Spores",
        "Sorcery",
        "Destroy target nonblack creature and target land. They can't be regenerated.",
    );
    let ids = rule["declaration"]["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|decision| decision["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["targetPermanent1", "targetPermanent2"]);
    assert_eq!(
        rule["effects"],
        json!([{
            "kind": "destroyPermanent",
            "permanent": {"kind": "union", "sets": [
                {"kind": "chosenTargets", "id": "targetPermanent1"},
                {"kind": "chosenTargets", "id": "targetPermanent2"},
            ]},
            "cannotRegenerate": true,
        }])
    );
    // Previously unused combination: two slots, a controller relation on
    // each, and the plural return destination.
    let rule = canonical(
        "Aether Tradewinds",
        "Instant",
        "Return target permanent you control and target permanent you don't control to their owners' hands.",
    );
    assert_eq!(rule["effects"][0]["kind"], "returnToOwnersHand");
    assert_eq!(rule["effects"][0]["object"]["kind"], "union");
    // A later `other target` slot is its own declaration; card composition
    // records that it must differ from the earlier slot (CR 115.3).
    let rule = canonical(
        "Other Target Probe",
        "Sorcery",
        "Destroy target artifact and up to one other target artifact.",
    );
    let decisions = rule["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[1]["minimum"], 0);
    assert_eq!(decisions[1]["distinctFrom"], json!([decisions[0]["id"]]));
    unsupported("Return target creature and target land to its owner's hand.");
}

#[test]
fn a_plural_regeneration_anaphor_needs_a_plural_destroyed_antecedent() {
    let rule = canonical(
        "Wrath of God",
        "Sorcery",
        "Destroy all creatures. They can't be regenerated.",
    );
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][0]["permanent"]["kind"], "eachPermanent");
    assert_eq!(rule["effects"][0]["cannotRegenerate"], true);
    unsupported("Destroy target creature. They can't be regenerated.");
    unsupported("Destroy all creatures. It can't be regenerated.");
}

#[test]
fn a_floating_each_distributes_only_a_plural_subject() {
    let rule = canonical(
        "Press the Advantage",
        "Instant",
        "Up to two target creatures each get +2/+2 and gain trample until end of turn.",
    );
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(
        (decision["minimum"].as_i64(), decision["maximum"].as_i64()),
        (Some(0), Some(2))
    );
    canonical(
        "Terrific Range Probe",
        "Instant",
        "One or two target creatures you control each get +1/+0 until end of turn.",
    );
    unsupported("Target creature each gets +2/+2 until end of turn.");
}

#[test]
fn a_counted_card_target_return_is_claimed_by_the_card_operand() {
    let rule = canonical(
        "Grave Return Probe",
        "Sorcery",
        "Return up to two target creature cards from your graveyard to your hand.",
    );
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["maximum"], 2);
    assert_eq!(decision["minimum"], 0);
}

#[test]
fn a_permission_owns_its_sentence_and_the_next_sentence_is_unconditional() {
    let rule = canonical(
        "Summon Undead",
        "Sorcery",
        "You may mill three cards. Then return a creature card from your graveyard to the battlefield.",
    );
    let effects = rule["effects"].as_array().unwrap();
    assert_eq!(effects[0]["kind"], "optionalEffects");
    assert_eq!(effects[0]["effects"].as_array().unwrap().len(), 1);
    assert!(effects.len() > 1, "the return is outside the permission");
}

fn target_list(text: &str) -> MachineResult<TargetListNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("target-list-tail", &tokens);
    let list = recognize_target_list(&mut cursor)?;
    cursor.finish()?;
    Ok(list)
}

#[test]
fn any_target_that_isnt_a_characteristic_is_a_player_or_permanent_union() {
    // CR 115.4: players stay candidates; damageable permanents lose the
    // excluded characteristic.
    let list = target_list("any target that isn't a Dragon").unwrap();
    let TargetSelector::Union(branches) = &list.slots[0].selector else {
        panic!("a player/permanent union: {list:?}");
    };
    assert_eq!(branches[0], TargetSelector::Player(PlayerPredicate::Any));
    let TargetSelector::Object(ObjectPredicate::Intersection(operands)) = &branches[1] else {
        panic!("a damageable permanent branch: {branches:?}");
    };
    assert!(matches!(operands[1], ObjectPredicate::Not(_)));
    // Sibling: the uncontracted copula reads the same exception.
    assert_eq!(
        target_list("any target that is not a Dragon").unwrap(),
        list
    );
    // Near miss: another controller in the excluded characteristic.
    assert!(target_list("any target that isn't a Dragon you control").is_err());
}

#[test]
fn a_set_constraint_needs_a_slot_that_may_declare_several_objects() {
    let list = target_list("two target creatures controlled by different players").unwrap();
    let TargetSelector::Object(predicate) = &list.slots[0].selector else {
        panic!("an object slot: {list:?}");
    };
    let (_, constraints) = predicate
        .split_set_constraints()
        .expect("a top-level set constraint");
    assert_eq!(constraints, vec![ObjectQualifier::DistinctControllers]);
    // Near miss: one declared object has no set to constrain.
    let tokens = recognize_entities("target creature controlled by different players");
    let mut cursor = OracleTokenCursor::new("target-list-set-near-miss", &tokens);
    let failure = recognize_target_list(&mut cursor).unwrap_err();
    assert_eq!(failure.state, "target-list.slot.set-constraint");
}
