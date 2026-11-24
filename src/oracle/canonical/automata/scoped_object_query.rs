//! Scoped access to the ordinary object query, independent of its consumer.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ScopedObjectQueryNode {
    pub domain: ObjectKind,
    pub objects: RelatedObjectQueryNode,
    pub zones: Vec<CardZoneName>,
    pub holders: Option<ZoneHolderQueryNode>,
    /// The query's source set is the departures of this turn instead of
    /// the objects now in `zones` (`creatures that died this turn`): no
    /// zone is read and `zones` is empty.
    pub departures: Option<DepartureHistory>,
}

/// The player a departure's relation names, relative to the ability
/// controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DeparturePlayer {
    AbilityController,
    OpponentOfAbilityController,
}

/// The departures of this turn a query reads (CR 700.4: an object dies when
/// it is put into a graveyard from the battlefield). The records keep the
/// last known characteristics of each object (CR 603.10a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DepartureHistory {
    /// `died under your control`: the player who controlled it as it died.
    pub controller: Option<DeparturePlayer>,
    /// `put into your graveyard`: the owner of the graveyard it went to,
    /// which is its owner (CR 404.2).
    pub graveyard_owner: Option<DeparturePlayer>,
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
    cursor.require(
        !objects.objects.exclude_source,
        "a zone collection without a source exclusion",
    )?;
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
        objects.objects.predicate.is_in_domain(domain),
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
                    | CardZoneName::Library
            )
        }),
        "a proved battlefield, graveyard, hand, exile or library query role",
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
        departures: None,
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
        // Printed characteristics of a card in a public or hidden zone are
        // its actual characteristics (CR 109.4 has no other source there).
        ObjectPredicate::Subtype(_)
        | ObjectPredicate::Color(_)
        | ObjectPredicate::Supertype(_)
        | ObjectPredicate::Relation(ObjectRelation::Named(_) | ObjectRelation::ChosenName) => true,
        ObjectPredicate::Relation(
            ObjectRelation::DealtDamageToPlayerThisTurn { .. }
            | ObjectRelation::Targets(_)
            | ObjectRelation::Related(_),
        ) => false,
        ObjectPredicate::IsCommander
        | ObjectPredicate::IsToken
        | ObjectPredicate::CharacteristicBatch(_)
        | ObjectPredicate::ColorQuantity(_)
        | ObjectPredicate::ExactManaCost(_)
        | ObjectPredicate::HasKeyword(_)
        | ObjectPredicate::Status(_)
        | ObjectPredicate::Designation(_)
        | ObjectPredicate::OwnedByAbilityController { .. }
        | ObjectPredicate::HasCounter(_)
        | ObjectPredicate::ChosenCreatureType
        | ObjectPredicate::ChosenColor
        | ObjectPredicate::ObjectKind(ObjectKind::Permanent | ObjectKind::Spell)
        | ObjectPredicate::NumericComparison { .. } => false,
    }
}

/// The object domains a lowered candidate query can name: permanents,
/// players, cards in a zone, spells. An unknown query names none.
pub(in crate::oracle::canonical) fn lowered_candidate_domains(
    candidates: &serde_json::Value,
) -> Vec<&'static str> {
    match candidates["kind"].as_str() {
        Some("permanents") => vec!["permanent"],
        Some("players") => vec!["player"],
        Some("anyTarget") => vec!["permanent", "player"],
        Some("cards") => vec!["card"],
        Some("spells" | "stackObjects") => vec!["spell"],
        Some("union") => candidates["sets"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(lowered_candidate_domains)
            .collect(),
        _ => Vec::new(),
    }
}

/// The type heads a lowered candidate query names positively, read with
/// the same hierarchy as `ObjectPredicate::positive_type_heads`: the
/// `where` filter's card types and subtypes through `and`/`or` only (a
/// negation names no head; a comparison operand, a counted collection or
/// any other relation names another object's heads, not the candidate's),
/// every set of a union, and the bare `permanent` noun for a permanent set
/// whose filter names no type (`target permanent`, `any target`).
pub(in crate::oracle::canonical) fn lowered_candidate_type_heads(
    candidates: &serde_json::Value,
) -> Vec<ObjectTypeHead> {
    fn filter_heads(filter: &serde_json::Value, heads: &mut Vec<ObjectTypeHead>) {
        match filter["kind"].as_str() {
            Some("cardTypeContains") => heads.extend(
                filter["value"]
                    .as_str()
                    .and_then(ObjectTypeHead::card_type_from_ir),
            ),
            Some("subtypeContains") => heads.extend(
                filter["value"]
                    .as_str()
                    .map(|name| ObjectTypeHead::Subtype(name.to_string())),
            ),
            Some("and" | "or") => filter["operands"]
                .as_array()
                .into_iter()
                .flatten()
                .for_each(|operand| filter_heads(operand, heads)),
            _ => {}
        }
    }
    match candidates["kind"].as_str() {
        Some("union") => candidates["sets"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(lowered_candidate_type_heads)
            .collect(),
        Some("permanents" | "anyTarget") => {
            let mut heads = Vec::new();
            filter_heads(&candidates["where"], &mut heads);
            if heads.is_empty() {
                // A permanent set without a type head is the bare
                // `permanent` noun; `any target` includes every permanent.
                heads.push(ObjectTypeHead::Permanent);
            }
            heads
        }
        _ => {
            let mut heads = Vec::new();
            filter_heads(&candidates["where"], &mut heads);
            heads
        }
    }
}

/// The type heads of a printed type line (`Legendary Creature — Dinosaur`):
/// its card types, its subtypes and, for a permanent card type, the
/// `permanent` noun. Supertypes name no head.
pub(in crate::oracle::canonical) fn type_line_type_heads(type_line: &str) -> Vec<ObjectTypeHead> {
    let mut parts = type_line.splitn(2, '\u{2014}');
    let types = parts.next().unwrap_or_default();
    let subtypes = parts.next().unwrap_or_default();
    let mut heads = types
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(ObjectTypeHead::card_type_from_ir)
        .collect::<Vec<_>>();
    if heads.iter().any(
        |head| matches!(head, ObjectTypeHead::CardType(card_type) if card_type.is_permanent_type()),
    ) {
        heads.push(ObjectTypeHead::Permanent);
    }
    heads.extend(
        subtypes
            .split_whitespace()
            .filter(|word| !word.is_empty())
            .map(|word| ObjectTypeHead::Subtype(word.to_string())),
    );
    heads
}

#[cfg(test)]
#[path = "scoped_object_query_tests.rs"]
mod tests;
