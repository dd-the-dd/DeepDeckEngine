//! Shared projection of object predicates and independent zone/player scope.
use super::super::*;

/// The same typed count as an ordinary action/static amount. `countCards`
/// is the zone count every numeric evaluator reads (stack resolution, static
/// characteristics and mana production); its holder is the ability
/// controller or every player. Any other holder or zone stays unlowered.
pub(in crate::oracle::canonical) fn lower_scoped_object_query_amount(
    query: &ScopedObjectQueryNode,
) -> Option<Value> {
    let [zone] = query.zones.as_slice() else {
        return None;
    };
    if query.domain != ObjectKind::Card
        || query.objects.objects.controller.is_some()
        || query.objects.ownership.is_some()
        || !matches!(
            zone,
            CardZoneName::Graveyard | CardZoneName::Hand | CardZoneName::Library
        )
    {
        return None;
    }
    let where_filter = lower_card_object_predicate(&query.objects.objects.predicate);
    let mut count = json!({
        "kind": "countCards",
        "zone": { "kind": zone.ir_name(), "player": controller() },
        "where": where_filter,
    });
    match query.holders.as_ref()? {
        ZoneHolderQueryNode::Player(LocalPlayerReference::Controller) => {}
        ZoneHolderQueryNode::Players(PlayerSetReferenceNode::EachPlayer) => {
            count["allPlayers"] = json!(true);
        }
        ZoneHolderQueryNode::Player(_) | ZoneHolderQueryNode::Players(_) => return None,
    }
    Some(count)
}

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
    if *zone == CardZoneName::Library {
        // The casting-cost collection reader has no library zone.
        return None;
    }
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
