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
fn additive_changes_are_independent_primitives_and_card_types_need_retention() {
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

    let replacing_card_type = node(
        Vec::new(),
        vec![ObjectCardType::Artifact],
        TypeChangeRetention::ReplacesSameClass,
    );
    assert!(lower_continuous_type_change(&objects, false, &replacing_card_type).is_err());
}
