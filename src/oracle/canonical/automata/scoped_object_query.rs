//! Scoped access to the ordinary object query, independent of its consumer.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ScopedObjectQueryNode {
    pub domain: ObjectKind,
    pub objects: RelatedObjectQueryNode,
    pub zones: Vec<CardZoneName>,
    pub holders: Option<ZoneHolderQueryNode>,
}

impl ScopedObjectQueryNode {
    pub(in crate::oracle::canonical) fn uses_x(&self) -> bool {
        object_predicate_uses_x(&self.objects.objects.predicate)
    }

    pub(in crate::oracle::canonical) fn requires_target_binding(&self) -> bool {
        fn requires(predicate: &ObjectPredicate) -> bool {
            match predicate {
                ObjectPredicate::NumericComparison { right, .. } => right.requires_target_binding(),
                ObjectPredicate::Not(child) => requires(child),
                ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
                    children.iter().any(requires)
                }
                _ => false,
            }
        }
        requires(&self.objects.objects.predicate)
    }
}

/// The predicate child owns coordinated heads and object relationships. This
/// wrapper owns zone-holder access, not card ownership or a new filter language.
pub(in crate::oracle::canonical) fn recognize_scoped_object_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ScopedObjectQueryNode> {
    let objects = recognize_related_object_collection(cursor)?;
    cursor.enter("query.zone.relation");
    let (domain, zones, holders) = if cursor
        .peek()
        .is_some_and(|word| word.eq_ignore_ascii_case("on"))
    {
        cursor.consume_phrase(&["on", "the"])?;
        let zone = recognize_card_zone_atom(cursor)?;
        cursor.require(
            zone == CardZoneName::Battlefield,
            "the battlefield after this zone relation",
        )?;
        (ObjectKind::Permanent, vec![zone], None)
    } else {
        cursor.consume("in")?;
        let holders = recognize_zone_holder_query(cursor)?;
        let zones = recognize_card_zone_list(cursor)?;
        (ObjectKind::Card, zones, Some(holders))
    };
    cursor.enter("query.zone.domain");
    cursor.require(
        objects.objects.predicate.supports_domain(domain)
            && objects.objects.predicate.has_domain(domain),
        "an explicit nominal domain appropriate to the query's actual zone",
    )?;
    cursor.require(
        crate::oracle::canonical::effects::domain_filter_is_representable(
            &objects.objects.predicate,
        ),
        "a Boolean query preserving its domain identities",
    )?;
    cursor.enter("query.zone.capability");
    cursor.require(
        zones.len() == 1,
        "one zone operand supported by the current checked query consumer",
    )?;
    cursor.require(
        zones.iter().all(|zone| {
            matches!(
                zone,
                CardZoneName::Battlefield
                    | CardZoneName::Graveyard
                    | CardZoneName::Hand
                    | CardZoneName::Exile
            )
        }),
        "a proved battlefield, graveyard, hand or exile query role",
    )?;
    // A controller is a live object relationship, not the player storing a
    // hand/graveyard. Do not reinterpret it as owner or holder.
    cursor.require(objects.objects.controller.is_none() && objects.ownership.is_none(),
        "a checked fact-bearing object relation for this query phase; holder is not controller or owner")?;
    if domain == ObjectKind::Card {
        cursor.enter("query.card-count.predicate-capability");
        cursor.require(
            card_count_predicate_supported(&objects.objects.predicate),
            "a card-type Boolean predicate in this checked count consumer",
        )?;
    }
    Ok(ScopedObjectQueryNode {
        domain,
        objects,
        zones,
        holders,
    })
}

/// Shared syntax is retained first; this consumer has not proved card P/T,
/// colors, nominal subtype identity or source-dependent characteristic reads.
fn card_count_predicate_supported(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::CardType(_)
        | ObjectPredicate::PermanentCard
        | ObjectPredicate::ObjectKind(ObjectKind::Card) => true,
        ObjectPredicate::Not(child) => card_count_predicate_supported(child),
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            !children.is_empty() && children.iter().all(card_count_predicate_supported)
        }
        ObjectPredicate::IsCommander
        | ObjectPredicate::IsToken
        | ObjectPredicate::Subtype(_)
        | ObjectPredicate::CharacteristicBatch(_)
        | ObjectPredicate::Color(_)
        | ObjectPredicate::ColorQuantity(_)
        | ObjectPredicate::Supertype(_)
        | ObjectPredicate::ExactManaCost(_)
        | ObjectPredicate::HasKeyword(_)
        | ObjectPredicate::Status(_)
        | ObjectPredicate::HasCounter(_)
        | ObjectPredicate::ChosenCreatureType
        | ObjectPredicate::ObjectKind(ObjectKind::Permanent | ObjectKind::Spell)
        | ObjectPredicate::NumericComparison { .. } => false,
    }
}

#[cfg(test)]
#[path = "scoped_object_query_tests.rs"]
mod tests;
