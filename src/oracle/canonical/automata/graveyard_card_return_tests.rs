use crate::oracle::syntax::recognize_entities;

use super::super::{ObjectCardType, ObjectKind};
use super::{LocalPlayerReference, ObjectPredicate, OracleTokenCursor};
use super::{OwnedCardZoneNode, recognize_return_target_card_from_own_graveyard};

#[test]
fn card_type_union_is_scoped_before_graveyard_and_hand_zones() {
    let tokens = recognize_entities(
        "Return target instant or sorcery card from your graveyard to your hand",
    );
    let mut cursor = OracleTokenCursor::new("graveyard-return-test", &tokens);
    let effect = recognize_return_target_card_from_own_graveyard(&mut cursor)
        .expect("graveyard return parses");

    assert_eq!(
        effect.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::ObjectKind(ObjectKind::Card),
            ObjectPredicate::Union(vec![
                ObjectPredicate::CardType(ObjectCardType::Instant),
                ObjectPredicate::CardType(ObjectCardType::Sorcery),
            ]),
        ])
    );
    assert_eq!(effect.owner, LocalPlayerReference::Controller);
    assert_eq!(effect.from, OwnedCardZoneNode::Graveyard);
    assert_eq!(effect.to, OwnedCardZoneNode::Hand);
    assert_eq!(cursor.position(), tokens.len());
}
