//! The one lowering of the condition language (`automata/condition.rs`) to
//! the engine's condition IR.
//!
//! A program position (a sentence `If`, an intervening `if`) lowers with the
//! object bindings proven at that position (`lower_sentence_condition`);
//! every other parent lowers without them (`lower_condition`). An atom that
//! needs a binding its parent cannot supply is an explicit failure (at a
//! program position) or an `unsupportedCondition` the engine refuses (where
//! the parent's allow-list should already have refused it).
use super::super::*;

pub(in crate::oracle::canonical) fn lower_choice_binding(binding: ChoiceBinding) -> &'static str {
    match binding {
        ChoiceBinding::BasicLandType => "basicLandType",
        ChoiceBinding::CardName => "chosenCardName",
        // Binding identifiers are opaque links, not a second representation
        // of the parsed numeric amount. `OptionalLifePaymentNode.amount` owns
        // the actual cost while this stable name links the choice to uses.
        ChoiceBinding::OptionalLifePayment => "optionalLifePayment",
    }
}

/// Object bindings visible at the condition's position in the sequence.
pub(in crate::oracle::canonical) struct ConditionObjectBindings<'a> {
    pub permanent_targets: &'a [(TargetBinding, String)],
    pub card_targets: &'a [(TargetBinding, String)],
    /// The singular card result of the current card-collection scope.
    pub card_focus: Option<Value>,
    /// The binding that records whether the instruction just before the
    /// condition was performed (`If you can't`), when the program has one.
    pub previous_action_result: Option<String>,
}

/// Whether the condition reads the result of the previous instruction.
pub(in crate::oracle::canonical) fn sentence_condition_reads_previous_action(
    condition: &ConditionNode,
) -> bool {
    condition_any(condition, &ConditionAtom::reads_previous_action)
}

/// Lowers a condition program of a parent without effect-program bindings
/// (static abilities, restrictions, family envelopes, `unless`).
pub(in crate::oracle::canonical) fn lower_condition(condition: &ConditionNode) -> Value {
    lower_condition_node(condition, None)
        .unwrap_or_else(|reason| json!({ "kind": "unsupportedCondition", "reason": reason }))
}

/// Lowers a condition program at an effect-program position, resolving
/// referenced objects to the declarations or bound results its symbolic
/// bindings name.
pub(in crate::oracle::canonical) fn lower_sentence_condition(
    cursor: &mut OracleTokenCursor<'_>,
    condition: &ConditionNode,
    bindings: &ConditionObjectBindings<'_>,
) -> MachineResult<Value> {
    lower_condition_node(condition, Some(bindings)).map_err(|reason| {
        cursor.enter("lower.condition");
        cursor.fail(reason)
    })
}

type Lowered = Result<Value, &'static str>;

fn lower_condition_node(
    condition: &ConditionNode,
    bindings: Option<&ConditionObjectBindings<'_>>,
) -> Lowered {
    Ok(match condition {
        BooleanConditionNode::Atom(atom) => lower_condition_atom(atom, bindings)?,
        BooleanConditionNode::And(operands) => and(operands
            .iter()
            .map(|operand| lower_condition_node(operand, bindings))
            .collect::<Result<Vec<_>, _>>()?),
        BooleanConditionNode::Or(operands) => or(operands
            .iter()
            .map(|operand| lower_condition_node(operand, bindings))
            .collect::<Result<Vec<_>, _>>()?),
        BooleanConditionNode::Not(operand) => negation(lower_condition_node(operand, bindings)?),
    })
}

fn lower_condition_atom(
    atom: &ConditionAtom,
    bindings: Option<&ConditionObjectBindings<'_>>,
) -> Lowered {
    match atom {
        ConditionAtom::Compare(comparison) => lower_comparison(comparison),
        ConditionAtom::ObjectSatisfies(node) => lower_object_condition(node, bindings),
        ConditionAtom::EventHistory(node) => Ok(lower_event_history(node)),
        ConditionAtom::CastFact(node) => lower_cast_fact(node),
        ConditionAtom::ActionOutcome(ActionOutcomeNode::PreviousActionImpossible) => {
            let binding = bindings
                .and_then(|bindings| bindings.previous_action_result.clone())
                .ok_or("a previous instruction whose action records whether it was performed")?;
            Ok(not(
                json!({ "kind": "bindingNotEmpty", "binding": binding }),
            ))
        }
        ConditionAtom::ActionOutcome(ActionOutcomeNode::CostNotPaid { payment }) => {
            Ok(not(json!({
                "kind": "costWasPaid",
                "decisionId": lower_choice_binding(*payment),
            })))
        }
        ConditionAtom::TurnOwnership(proposition) => Ok(if proposition.is_controller_turn {
            json!({ "kind": "duringControllerTurn", "player": controller() })
        } else {
            json!({ "kind": "notYourTurn" })
        }),
        ConditionAtom::PlayerDesignation(node) => Ok(lower_player_designation(node)),
    }
}

fn lower_player_designation(node: &PlayerDesignationNode) -> Value {
    let atom = match &node.designation {
        PlayerDesignation::EnduringStory => {
            json!({ "kind": "hasEnduringStory", "player": controller() })
        }
        PlayerDesignation::CompletedDungeon { dungeon } => {
            let mut atom = json!({ "kind": "completedDungeon", "player": controller() });
            if let Some(dungeon) = dungeon {
                atom["name"] = json!(dungeon);
            }
            atom
        }
    };
    if node.held { atom } else { not(atom) }
}

/// A cast fact of the ability's own source, or the mana spent to cast the
/// spell a cast event observed.
fn lower_cast_fact(node: &CastFactNode) -> Lowered {
    if let CastFact::ManaSpent { amount, negated } = node.fact {
        let spell = match node.spell {
            CastSpellReference::Source(_) => self_ref(),
            CastSpellReference::EventSpell => json!({ "kind": "triggeringSpell" }),
        };
        let spent = |symbol: Option<&str>| {
            let mut amount = json!({ "kind": "manaSpentToCast", "spell": spell.clone() });
            if let Some(symbol) = symbol {
                amount["symbol"] = json!(symbol);
            }
            amount
        };
        let fact = match amount {
            ManaSpentAmount::Symbols(counts) => {
                let mut atoms = ManaSpentAmount::SYMBOLS
                    .iter()
                    .zip(counts)
                    .filter(|(_, count)| *count > 0)
                    .map(|(symbol, count)| {
                        compare(">=", spent(Some(*symbol)), integer(i64::from(count)))
                    })
                    .collect::<Vec<_>>();
                match atoms.len() {
                    0 => return Err("at least one spent mana symbol"),
                    1 => atoms.remove(0),
                    _ => and(atoms),
                }
            }
            ManaSpentAmount::Total { operator, value } => {
                compare(operator.ir_operator(), spent(None), integer(value))
            }
        };
        return Ok(if negated { not(fact) } else { fact });
    }
    if node.spell != CastSpellReference::Source(LocalObjectReference::SourceSelf) {
        return Err("a cast fact of the ability's own source");
    }
    Ok(match node.fact {
        CastFact::ManaSpent { .. } => unreachable!("the mana spent was lowered above"),
        CastFact::Kicked => json!({ "kind": "wasKicked", "spell": self_ref() }),
        CastFact::Cast { zone, negated } => {
            let fact = match zone {
                None => json!({ "kind": "wasCast", "object": self_ref() }),
                Some(SourceCastZone::Hand) => {
                    json!({ "kind": "wasCastFromHand", "object": self_ref() })
                }
                Some(SourceCastZone::Graveyard) => json!({
                    "kind": "wasCastFromZone", "object": self_ref(), "zone": "graveyard",
                }),
                Some(SourceCastZone::Exile) => json!({
                    "kind": "wasCastFromZone", "object": self_ref(), "zone": "exile",
                }),
            };
            if negated { not(fact) } else { fact }
        }
    })
}

/// `Compare(left, op, right)` for its player subject. A quantified subject
/// wraps the same comparison in `playerQuantifier`, whose inner condition
/// names the member with `quantifiedPlayer` (CR 102.2-102.3).
fn lower_comparison(comparison: &ConditionComparison) -> Lowered {
    let operator = comparison.operator.ir_operator();
    match comparison.subject {
        ConditionPlayerSubject::Controller => Ok(compare(
            operator,
            lower_condition_number(&comparison.left, controller())?,
            lower_condition_number(&comparison.right, controller())?,
        )),
        // `that player ...`: the player the triggering event records.
        ConditionPlayerSubject::EventPlayer => {
            let player = || json!({ "kind": "triggeringPlayer" });
            Ok(compare(
                operator,
                lower_condition_number(&comparison.left, player())?,
                lower_condition_number(&comparison.right, player())?,
            ))
        }
        ConditionPlayerSubject::OpponentsTogether => {
            // `your opponents control N QUERY`: one count over every opponent.
            let (
                ConditionNumber::Player(PlayerStateMeasure::ControlledPermanents {
                    predicate, ..
                }),
                ConditionNumber::Integer(value),
            ) = (&comparison.left, &comparison.right)
            else {
                return Err("a controlled-permanent count of the opponents together");
            };
            Ok(compare(
                operator,
                json!({
                    "kind": "countPermanentsControlledByOpponents",
                    "player": controller(),
                    "where": lower_permanent_object_predicate(predicate),
                }),
                integer(*value),
            ))
        }
        ConditionPlayerSubject::Quantified {
            quantifier,
            population,
        } => {
            let players = match population {
                PlayerPopulation::Opponents => {
                    json!({ "kind": "opponentsOf", "player": controller() })
                }
                PlayerPopulation::Players => json!({ "kind": "eachPlayer" }),
            };
            let member = || json!({ "kind": "quantifiedPlayer" });
            let inner = compare(
                operator,
                lower_condition_number(&comparison.left, member())?,
                lower_condition_number(&comparison.right, member())?,
            );
            let quantified = |quantifier: &str| {
                json!({
                    "kind": "playerQuantifier",
                    "quantifier": quantifier,
                    "players": players,
                    "condition": inner,
                })
            };
            Ok(match quantifier {
                PlayerQuantifier::Some => quantified("some"),
                PlayerQuantifier::Every => quantified("every"),
                // `no opponent ...` is the negation of `an opponent ...`.
                PlayerQuantifier::None => not(quantified("some")),
            })
        }
    }
}

fn lower_condition_number(number: &ConditionNumber, player: Value) -> Lowered {
    match number {
        ConditionNumber::Integer(value) => Ok(integer(*value)),
        ConditionNumber::Expression(expression) => lower_numeric_expression(expression)
            .ok_or("a numeric expression without a target binding"),
        ConditionNumber::Player(measure) => Ok(lower_player_measure(measure, player)),
        ConditionNumber::ControllerMeasure(measure) => {
            Ok(lower_player_measure(measure, controller()))
        }
        ConditionNumber::GreatestOpponent(PlayerStateMeasure::ControlledPermanents {
            predicate,
            ..
        }) => Ok(json!({
            "kind": "greatestOpponentPermanentCount",
            "player": player,
            "where": lower_permanent_object_predicate(predicate),
        })),
        ConditionNumber::GreatestOpponent(_) => {
            Err("the greatest opponent value of a measure other than controlled permanents")
        }
    }
}

/// The recorded event kind(s) of a history pattern.
fn history_event_kind(event: HistoryEvent) -> Value {
    let kind = match event {
        HistoryEvent::SpellCast => "spellCast",
        HistoryEvent::CardDrawn => "cardDrawn",
        HistoryEvent::LifeGained => "lifeGained",
        HistoryEvent::LifeLost => "lifeLost",
        HistoryEvent::LifeChanged => return json!(["lifeGained", "lifeLost"]),
        HistoryEvent::DamageDealtToPlayer => "damageDealt",
        HistoryEvent::AttackerDeclared => "attackerDeclared",
        HistoryEvent::PermanentDied => "permanentDied",
        HistoryEvent::PermanentLeftBattlefield => "permanentLeftBattlefield",
        HistoryEvent::PermanentEntered => "permanentEnteredBattlefield",
        HistoryEvent::LibrarySearched => "librarySearched",
        HistoryEvent::PermanentSacrificed => "permanentSacrificed",
        HistoryEvent::CardDiscarded => "cardDiscarded",
        HistoryEvent::CardCycled => "cardCycled",
        HistoryEvent::TokenCreated => "tokenCreated",
        HistoryEvent::CountersPlaced => "countersPlaced",
        HistoryEvent::LandPlayed => "landPlayed",
        HistoryEvent::CardsPutIntoGraveyard => "cardsEnteredGraveyard",
    };
    json!(kind)
}

/// The player relation of a history player, relative to the controller.
fn history_player_relation(player: HistoryPlayer, event: HistoryEvent) -> Option<Value> {
    match player {
        // Damage to a player: an event of any player, never of a creature.
        HistoryPlayer::Any if event == HistoryEvent::DamageDealtToPlayer => {
            Some(json!({ "kind": "eachPlayer" }))
        }
        HistoryPlayer::Any => None,
        HistoryPlayer::Controller => Some(controller()),
        HistoryPlayer::Opponent => Some(json!({ "kind": "opponentsOf", "player": controller() })),
    }
}

/// `EventHistory(Pattern, scope)` as the generic event count of the engine
/// (`countEventsThisTurn` / `countEventsPreviousTurn`) compared with its
/// threshold. The controller's life this turn is the shared numeric fact of
/// that player (`lifeGainedThisTurn`, `lifeLostThisTurn`).
fn lower_event_history(node: &EventHistoryNode) -> Value {
    let pattern = &node.pattern;
    let operator = node.operator.ir_operator();
    if pattern.player == HistoryPlayer::Controller
        && pattern.object.is_none()
        && pattern.measure == HistoryMeasure::Amount
        && node.scope == HistoryScope::ThisTurn
        && node.aggregate == HistoryAggregate::Total
        && pattern.qualifiers == HistoryQualifiers::default()
    {
        let fact = match pattern.event {
            HistoryEvent::LifeGained => Some("lifeGainedThisTurn"),
            HistoryEvent::LifeLost => Some("lifeLostThisTurn"),
            _ => None,
        };
        if let Some(fact) = fact {
            return compare(
                operator,
                json!({ "kind": fact, "player": controller() }),
                integer(node.threshold),
            );
        }
    }
    let mut count = json!({
        "kind": match node.scope {
            HistoryScope::ThisTurn => "countEventsThisTurn",
            HistoryScope::LastTurn => "countEventsPreviousTurn",
        },
        "event": history_event_kind(pattern.event),
    });
    let qualifiers = &pattern.qualifiers;
    let player = history_player_relation(pattern.player, pattern.event);
    if pattern.event == HistoryEvent::CountersPlaced {
        // The placement's player is the permanent's controller; the actor
        // is the recorded placer.
        if let Some(actor) = player {
            count["relatedPlayers"] = json!({ "placedByPlayerId": actor });
        }
        if let Some(recipient) = qualifiers
            .recipient_controller
            .and_then(|recipient| history_player_relation(recipient, pattern.event))
        {
            count["player"] = recipient;
        }
    } else if let Some(player) = player {
        count["player"] = player;
    }
    if qualifiers.zone_owner {
        // CR 404.2: the departing card's owner, not its controller.
        count["relation"] = json!("owner");
    }
    let filter = pattern
        .object
        .as_ref()
        .map(|predicate| match pattern.event {
            HistoryEvent::SpellCast => {
                lower_spell_object_predicate(predicate).unwrap_or(Value::Null)
            }
            HistoryEvent::CardDiscarded
            | HistoryEvent::CardCycled
            | HistoryEvent::CardDrawn
            | HistoryEvent::LandPlayed
            | HistoryEvent::CardsPutIntoGraveyard => lower_card_object_predicate(predicate),
            _ => lower_permanent_object_predicate(predicate),
        });
    match node.scope {
        HistoryScope::ThisTurn => count["where"] = filter.unwrap_or(Value::Null),
        HistoryScope::LastTurn => {
            if let Some(filter) = filter {
                count["where"] = filter;
            }
        }
    }
    let mut detail = serde_json::Map::new();
    if let Some(counter) = &qualifiers.counter {
        detail.insert("counter".to_string(), json!(counter.as_str()));
    }
    if let Some(HistoryDestination::Hand) = qualifiers.destination {
        detail.insert("destination".to_string(), json!("hand"));
    }
    if let Some(HistoryCastMode::Warp) = qualifiers.cast_mode {
        // A JSON pointer into the cast record's decisions.
        detail.insert("/decisions/useWarp".to_string(), json!(true));
    }
    if !detail.is_empty() {
        count["detail"] = Value::Object(detail);
    }
    if qualifiers.exclude_source {
        count["excludeObject"] = json!({ "kind": "self" });
    }
    match pattern.measure {
        HistoryMeasure::Occurrences => {}
        HistoryMeasure::Amount => count["measure"] = json!("amount"),
        HistoryMeasure::Cards => count["measure"] = json!("cards"),
    }
    if node.scope == HistoryScope::LastTurn || node.aggregate == HistoryAggregate::GreatestPerPlayer
    {
        count["aggregate"] = json!(match node.aggregate {
            HistoryAggregate::Total => "total",
            HistoryAggregate::GreatestPerPlayer => "greatestPerPlayer",
        });
    }
    compare(operator, count, integer(node.threshold))
}

/// An object condition. Without program bindings, the only object is the
/// ability's source; its tapped, combat, modified and designation states
/// lower to the continuous evaluator's source-state kinds.
fn lower_object_condition(
    node: &ReferencedObjectConditionNode,
    bindings: Option<&ConditionObjectBindings<'_>>,
) -> Lowered {
    let Some(bindings) = bindings else {
        if node.object != ConditionObjectReference::SOURCE {
            return Err("an object binding of an effect program");
        }
        let source = || json!({ "kind": "isTapped", "object": self_ref() });
        let state = match &node.predicate {
            ObjectPredicate::Status(ObjectStatus::Tapped) => Some(source()),
            ObjectPredicate::Status(ObjectStatus::Untapped) => Some(not(source())),
            ObjectPredicate::Status(ObjectStatus::Attacking) => {
                Some(json!({ "kind": "isAttacking", "object": self_ref() }))
            }
            ObjectPredicate::Status(ObjectStatus::Blocking) => {
                Some(json!({ "kind": "isBlocking", "object": self_ref() }))
            }
            ObjectPredicate::Status(ObjectStatus::Modified) => {
                Some(json!({ "kind": "sourceIsModified" }))
            }
            ObjectPredicate::Designation(designation) => Some(json!({
                "kind": "sourceFlagSet",
                "flag": designation.flag(),
            })),
            _ => None,
        };
        if let Some(state) = state {
            return Ok(state);
        }
        let filter = lower_permanent_object_predicate(&node.predicate);
        if filter.is_null() {
            return Err("a characteristic that lowers to a nonempty object filter");
        }
        return Ok(json!({ "kind": "objectMatchesFilter", "object": self_ref(), "where": filter }));
    };
    let (object, filter) = match node.object {
        ConditionObjectReference::Permanent(reference) => (
            bound_permanent_reference_value(reference, bindings.permanent_targets)
                .ok_or("the previously declared permanent target slot in this lexical scope")?,
            lower_permanent_object_predicate(&node.predicate),
        ),
        ConditionObjectReference::CardTarget(binding) => (
            bindings
                .card_targets
                .iter()
                .find_map(|(candidate, id)| (*candidate == binding).then(|| chosen_target(id)))
                .ok_or("the singular card target declared earlier in this sequence")?,
            lower_card_object_predicate(&node.predicate),
        ),
        ConditionObjectReference::CardFocus => (
            bindings
                .card_focus
                .clone()
                .ok_or("a singular card result in the current card-collection scope")?,
            lower_card_object_predicate(&node.predicate),
        ),
    };
    if filter.is_null() {
        return Err("a characteristic that lowers to a nonempty object filter");
    }
    Ok(json!({ "kind": "objectMatchesFilter", "object": object, "where": filter }))
}
