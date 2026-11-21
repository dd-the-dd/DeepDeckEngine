//! Shared projection of object predicates and independent zone/player scope.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_scoped_object_query_count(
    query: &ScopedObjectQueryNode,
) -> Option<Value> {
    let [zone] = query.zones.as_slice() else {
        return None;
    };
    if query.objects.objects.controller.is_some() || query.objects.ownership.is_some() {
        return None;
    }
    let where_filter = match query.domain {
        ObjectKind::Permanent => lower_permanent_object_predicate(&query.objects.objects.predicate),
        ObjectKind::Card => lower_card_object_predicate(&query.objects.objects.predicate),
        ObjectKind::Spell => return None,
    };
    if *zone == CardZoneName::Battlefield {
        // Compatibility is a projection of this same typed query, not a
        // second predicate evaluator or a card-specific count operation.
        Some(json!({"kind":"countPermanents","allPlayers":true,"where":where_filter}))
    } else {
        let mut zone_ref = json!({"kind":zone.ir_name()});
        match query.holders.as_ref()? {
            ZoneHolderQueryNode::Player(LocalPlayerReference::Controller) => {
                zone_ref["player"] = controller()
            }
            ZoneHolderQueryNode::Players(players) => {
                zone_ref["players"] = lower_player_set_reference(*players)
            }
            ZoneHolderQueryNode::Player(
                LocalPlayerReference::ControllerOf(_)
                | LocalPlayerReference::ControllerOfQualified(_),
            ) => return None,
        }
        Some(
            json!({"kind":"countObjects","objects":{"kind":"cardsInZone","zone":zone_ref,"where":where_filter}}),
        )
    }
}
