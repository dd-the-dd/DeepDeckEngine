//! Lowering of the performers whose members an earlier action of the
//! program determines (CR 608.2c): a population restricted by its members'
//! performance of the immediately preceding population action (`Each player
//! may shuffle ... . Each player who does draws seven cards.`).
//!
//! The effect-sequence lowering delegates to [`lower_derived_performer`]
//! before its own performer lowering; every other performer keeps that
//! lowering. The producer's player loop records which members performed its
//! instruction (`forEachPlayer.record`, measured by the engine's
//! performance witnesses, CR 609.3), and the consumer's loop iterates the
//! members of its population that the record keeps (`recordedPlayers`).
use super::super::*;

/// The binding the producer's player loop records its performing members
/// under.
pub(in crate::oracle::canonical) fn performance_record_id(producer: usize) -> String {
    format!("performedPlayers{producer}")
}

/// Lowers the performer of the action whose primitives start at `start` of
/// `effects` when this module owns it; `None` for every other performer.
/// The caller wraps a failure as its own unsupported support limit.
pub(in crate::oracle::canonical) fn lower_derived_performer(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut Vec<Value>,
    start: usize,
    effect_index: usize,
    performer: &PerformerNode,
) -> Option<MachineResult<()>> {
    let PerformerNode::FilteredPopulation(query) = performer else {
        return None;
    };
    let Some(PlayerRelativePredicate::Performance {
        producer,
        performed,
    }) = &query.predicate
    else {
        return None;
    };
    Some(lower_performance_population(
        cursor,
        effects,
        PerformanceConsumer {
            start,
            effect_index,
            query,
            producer: *producer,
            performed: *performed,
        },
    ))
}

/// The action restricted by its members' performance, and the producer it
/// observes.
struct PerformanceConsumer<'a> {
    start: usize,
    effect_index: usize,
    query: &'a PlayerPopulationQueryNode,
    producer: usize,
    performed: bool,
}

/// `each player who does|doesn't|can't ACTION`: the producer's player loop
/// (`performer{producer}`, lowered earlier in this program) records its
/// performing members, and every member of the consumer's population whose
/// recorded performance is `performed` performs the action in turn order
/// (CR 101.4). A producer the program did not lower as one player loop is
/// an explicit failure.
fn lower_performance_population(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut Vec<Value>,
    consumer: PerformanceConsumer<'_>,
) -> MachineResult<()> {
    let PerformanceConsumer {
        start,
        effect_index,
        query,
        producer,
        performed,
    } = consumer;
    cursor.enter("lower.performer.performance-population");
    cursor.require(
        query.except.is_none() && !query.unique,
        "a population restricted only by its members' performance of the preceding action",
    )?;
    cursor.require(
        effects.len() > start,
        "an action that produces at least one primitive for its performer",
    )?;
    let record = performance_record_id(producer);
    cursor.enter("lower.performer.performance-population.producer");
    // The producer is the action immediately before this one: its lowering
    // ends right before this action's primitives, with its player loop.
    cursor.require(
        producer + 1 == effect_index,
        "the population action immediately before the relative's action",
    )?;
    {
        let producer_loop = start
            .checked_sub(1)
            .and_then(|last| effects.get_mut(last))
            .filter(|effect| {
                effect["kind"] == "forEachPlayer"
                    && effect
                        .get("record")
                        .is_none_or(|written| written.as_str() == Some(record.as_str()))
            });
        let producer_loop = cursor.require_value(
            producer_loop,
            "the lowered player loop of the population action whose performance the relative observes",
        )?;
        producer_loop["record"] = json!(record);
    }
    // CR 102.3: `each other player` is every player but the controller.
    let players = if query.excluding_controller {
        json!({"kind": "playersExcept", "player": controller()})
    } else {
        lower_player_set_reference(query.players)
    };
    let bind = format!("performer{effect_index}");
    let member = json!({"kind": "boundValue", "id": bind});
    let body = effects
        .drain(start..)
        .map(|mut effect| {
            effect["performer"] = member.clone();
            effect
        })
        .collect::<Vec<_>>();
    let collect = super::resolution_object_choice::player_loop_collection(&body);
    let mut lowered = json!({
        "kind": "forEachPlayer",
        "bind": bind,
        "players": {
            "kind": "recordedPlayers",
            "players": players,
            "record": record,
            "performed": performed,
        },
        "effects": body,
    });
    if let Some(collect) = collect {
        lowered["collect"] = collect;
    }
    effects.push(lowered);
    Ok(())
}
