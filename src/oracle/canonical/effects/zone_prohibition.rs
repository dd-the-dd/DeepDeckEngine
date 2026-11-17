use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

pub(in crate::oracle::canonical) enum ZoneProhibitionProgramAttempt {
    Parsed(CanonicalRuleDraft),
    NotClaimed,
    Failed(MachineFailure),
}

// The existing domain lowerer erases a bare card/spell identity. It does not
// yet represent TRUE beneath OR or NOT. Keep that historical contract bounded
// instead of turning a universal branch into a narrower filter.
fn is_unfiltered_domain(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::ObjectKind(_) => true,
        ObjectPredicate::Intersection(children) => children.iter().all(is_unfiltered_domain),
        _ => false,
    }
}
pub(in crate::oracle::canonical) fn domain_filter_is_representable(
    predicate: &ObjectPredicate,
) -> bool {
    match predicate {
        ObjectPredicate::Union(children) => children
            .iter()
            .all(|child| !is_unfiltered_domain(child) && domain_filter_is_representable(child)),
        ObjectPredicate::Not(child) => {
            !is_unfiltered_domain(child) && domain_filter_is_representable(child)
        }
        ObjectPredicate::Intersection(children) => {
            children.iter().all(domain_filter_is_representable)
        }
        _ => true,
    }
}
pub(in crate::oracle::canonical) fn try_parse_promoted_zone_prohibition(
    input: &AbilityInput<'_>,
) -> ZoneProhibitionProgramAttempt {
    let mut cursor = OracleTokenCursor::new("static-zone-prohibition", input.tokens);
    let node = match try_recognize_zone_prohibition(&mut cursor) {
        ZoneProhibitionAttempt::Parsed(node) => node,
        ZoneProhibitionAttempt::NotClaimed | ZoneProhibitionAttempt::UnsupportedEnvelope(_) => {
            return ZoneProhibitionProgramAttempt::NotClaimed;
        }
        ZoneProhibitionAttempt::Failed(failure) => {
            return ZoneProhibitionProgramAttempt::Failed(failure);
        }
    };
    match lower_zone_prohibition(&mut cursor, node, input.face_type_line) {
        Ok(draft) => ZoneProhibitionProgramAttempt::Parsed(draft),
        Err(failure) => ZoneProhibitionProgramAttempt::Failed(failure),
    }
}
fn lower_zone_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
    node: ZoneProhibitionNode,
    type_line: &str,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("static.zone-prohibition.source-context");
    cursor.require(
        type_line_has_permanent_card_type(type_line),
        "a permanent source face",
    )?;
    let modifier = match node {
        ZoneProhibitionNode::Cast {
            players,
            spells,
            zones,
        } => {
            cursor.enter("static.zone-prohibition.cast.filter-support");
            cursor.require(
                domain_filter_is_representable(&spells.predicate),
                "a filter whose Boolean branches preserve the shared domain lowerer's identities",
            )?;
            cursor.enter("static.zone-prohibition.cast.players");
            cursor.require(
                players == PlayerSetReferenceNode::EachPlayer,
                "all players; narrower runtime zone-prohibition scopes are not yet implemented",
            )?;
            cursor.enter("static.zone-prohibition.cast.caster");
            cursor.require(
                spells.caster.is_none(),
                "a spell collection without an additional caster qualifier",
            )?;
            cursor.enter("static.zone-prohibition.cast.zones");
            cursor.require(
                zones.iter().all(|z| !matches!(z, CardZoneName::Hand)),
                "graveyard, library or exile casting zones",
            )?;
            json!({"kind":"prohibitCastFromZones","players":{"kind":"eachPlayer"},"zones":zones.into_iter().map(CardZoneName::ir_name).collect::<Vec<_>>(),"where":lower_spell_object_predicate(&spells.predicate).unwrap_or(Value::Null)})
        }
        ZoneProhibitionNode::Enter { cards, zones } => {
            cursor.enter("static.zone-prohibition.entry.filter-support");
            cursor.require(
                domain_filter_is_representable(&cards.predicate),
                "a filter whose Boolean branches preserve the shared domain lowerer's identities",
            )?;
            cursor.enter("static.zone-prohibition.entry.owners");
            cursor.require(
                cards.controller.is_none(),
                "an unqualified card population across the listed zones",
            )?;
            cursor.enter("static.zone-prohibition.entry.zones");
            cursor.require(
                zones
                    .iter()
                    .all(|z| matches!(z, CardZoneName::Graveyard | CardZoneName::Library)),
                "graveyard or library entry zones; other entry families remain explicitly capped",
            )?;
            json!({"kind":"prohibitEnterBattlefieldFromZones","zones":zones.into_iter().map(CardZoneName::ir_name).collect::<Vec<_>>(),"where":lower_card_object_predicate(&cards.predicate)})
        }
    };
    Ok(draft(
        json!({"kind":"staticAbility","source":self_ref(),"activeWhile":active_while_battlefield(),"modifiers":[modifier]}),
        &[
            "Compose a typed card/spell query, zone union and prohibition",
            "Preserve source-zone and player scope in an atomic modifier",
        ],
    ))
}

#[cfg(test)]
#[path = "zone_prohibition_tests.rs"]
mod tests;
