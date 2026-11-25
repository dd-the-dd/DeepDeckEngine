use serde_json::json;

use crate::oracle::characteristic_vocabulary::BasicLandSubtype;

use super::super::super::*;
use super::lower_continuous_type_change;

fn node(
    subtypes: Vec<TypeChangeSubtype>,
    card_types: Vec<ObjectCardType>,
    retention: TypeChangeRetention,
) -> TypeChangeCharacteristicsNode {
    TypeChangeCharacteristicsNode {
        supertypes: Vec::new(),
        subtypes,
        card_types,
        all_basic_land_types: false,
        retention,
    }
}

#[test]
fn replacing_land_subtype_requires_proved_land_objects() {
    let objects = json!({ "kind": "permanents" });
    let mountains = node(
        vec![TypeChangeSubtype::BasicLand(BasicLandSubtype::Mountain)],
        Vec::new(),
        TypeChangeRetention::ReplacesSameClass,
    );
    let lowered = lower_continuous_type_change(&objects, true, &mountains).expect("lands");
    assert_eq!(
        lowered,
        [json!({ "kind": "setLandSubtype", "objects": objects, "subtype": "Mountain" })]
    );
    assert!(lower_continuous_type_change(&objects, false, &mountains).is_err());
}

#[test]
fn additive_changes_are_independent_primitives() {
    let objects = json!({ "kind": "attachedPermanent", "attachment": { "kind": "self" } });
    let forest_lands = node(
        vec![TypeChangeSubtype::BasicLand(BasicLandSubtype::Forest)],
        vec![ObjectCardType::Land],
        TypeChangeRetention::InAdditionToOtherTypes,
    );
    let kinds = lower_continuous_type_change(&objects, false, &forest_lands)
        .expect("additive")
        .iter()
        .map(|modifier| modifier["kind"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["addCardType", "addSubtype"]);
}

/// CR 205.1a: card types named without retention replace the card types;
/// CR 205.1b: an `artifact creature` change retains them.
#[test]
fn replacing_card_types_set_them_and_artifact_creature_retains() {
    let objects = json!({ "kind": "attachedPermanent", "attachment": { "kind": "self" } });
    let artifact = node(
        Vec::new(),
        vec![ObjectCardType::Artifact],
        TypeChangeRetention::ReplacesSameClass,
    );
    assert_eq!(
        lower_continuous_type_change(&objects, false, &artifact).expect("set"),
        [json!({ "kind": "setCardTypes", "objects": objects, "cardTypes": ["Artifact"] })]
    );
    let artifact_creature = node(
        Vec::new(),
        vec![ObjectCardType::Artifact, ObjectCardType::Creature],
        TypeChangeRetention::ReplacesSameClass,
    );
    let kinds = lower_continuous_type_change(&objects, false, &artifact_creature)
        .expect("retained")
        .iter()
        .map(|modifier| modifier["kind"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["addCardType", "addCardType"]);
}

/// A set to `land` proves the objects are lands, so one basic land type
/// replaces their land types (CR 305.7); without that proof it fails.
#[test]
fn land_card_type_set_proves_the_replacing_land_subtype() {
    let objects = json!({ "kind": "attachedPermanent", "attachment": { "kind": "self" } });
    let forest_land = node(
        vec![TypeChangeSubtype::BasicLand(BasicLandSubtype::Forest)],
        vec![ObjectCardType::Land],
        TypeChangeRetention::ReplacesSameClass,
    );
    assert_eq!(
        lower_continuous_type_change(&objects, false, &forest_land).expect("land set"),
        [
            json!({ "kind": "setCardTypes", "objects": objects, "cardTypes": ["Land"] }),
            json!({ "kind": "setLandSubtype", "objects": objects, "subtype": "Forest" }),
        ]
    );
    let forest_creature = node(
        vec![TypeChangeSubtype::BasicLand(BasicLandSubtype::Forest)],
        vec![ObjectCardType::Creature],
        TypeChangeRetention::ReplacesSameClass,
    );
    assert!(lower_continuous_type_change(&objects, true, &forest_creature).is_err());
}
