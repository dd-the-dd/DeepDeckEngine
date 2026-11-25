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
        ConditionAtom::Compare(comparison) => lower_comparison(comparison, bindings),
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
        ConditionAtom::PlayerTurn(node) => lower_player_turn(node),
        ConditionAtom::PlayerDesignation(node) => lower_player_designation(node),
        ConditionAtom::ObjectZone(node) => lower_object_zone(node, bindings),
        ConditionAtom::ObjectControl(node) => lower_object_control(node, bindings),
        ConditionAtom::CastChoice(node) => Ok(lower_cast_choice(node)),
        ConditionAtom::SpellCastOrdinal(node) => lower_spell_cast_ordinal(node),
        ConditionAtom::TopCard(node) => lower_top_card(node),
        ConditionAtom::MostCommonColor(node) => lower_most_common_color(node),
    }
}

/// `castChoiceMade { spell: self, choice, mode?, manaCost? }`: a choice made
/// for the ability's own source as it was cast (CR 601.2b-f), read on the
/// resolving spell's decisions or on the record of the permanent it became.
fn lower_cast_choice(node: &CastChoiceNode) -> Value {
    let mut atom = json!({ "kind": "castChoiceMade", "spell": self_ref() });
    match &node.choice {
        CastChoice::GiftPromised => atom["choice"] = json!("gift"),
        CastChoice::Teamwork => atom["choice"] = json!("teamwork"),
        CastChoice::AlternativeMode(mode) => {
            atom["choice"] = json!("alternativeCost");
            atom["mode"] = json!(mode.ir_name());
        }
        CastChoice::AlternativeManaCost(cost) => {
            atom["choice"] = json!("alternativeCost");
            atom["manaCost"] = json!(cost);
        }
    }
    if node.negated { not(atom) } else { atom }
}

/// `spellCastOrdinal { spell, ordinal, where? }`: the observed spell is its
/// caster's ORDINAL spell of the query this turn (CR 601.2i), counted up to
/// its own cast record.
fn lower_spell_cast_ordinal(node: &SpellCastOrdinalNode) -> Lowered {
    if node.spell != CastSpellReference::EventSpell {
        return Err("the spell a cast event observed");
    }
    let mut atom = json!({
        "kind": "spellCastOrdinal",
        "spell": { "kind": "triggeringSpell" },
        "ordinal": node.ordinal,
    });
    if let Some(predicate) = &node.predicate {
        atom["where"] = lower_spell_object_predicate(predicate)
            .ok_or("a spell predicate the cast record can evaluate")?;
    }
    Ok(atom)
}

/// `topCardMatches { player, zone, where }` (CR 401.1, 404.1).
fn lower_top_card(node: &TopCardConditionNode) -> Lowered {
    if node.player != ConditionPlayerSubject::Controller {
        return Err("the top card of the ability controller's own zone");
    }
    let filter = lower_card_object_predicate(&node.predicate);
    if filter.is_null() {
        return Err("a card characteristic that lowers to a nonempty filter");
    }
    Ok(json!({
        "kind": "topCardMatches",
        "player": controller(),
        "zone": node.zone.ir_name(),
        "where": filter,
    }))
}

/// `colorIsMostCommon { color, where, tieAllowed }` (CR 105.1): the color's
/// count among the permanents of `where` is at least every other color's
/// (strictly greater without `tieAllowed`).
fn lower_most_common_color(node: &MostCommonColorNode) -> Lowered {
    Ok(json!({
        "kind": "colorIsMostCommon",
        "color": canonical_color_name(node.color),
        "where": lower_permanent_object_predicate(&node.predicate),
        "tieAllowed": node.tie_allowed,
    }))
}

/// `you [didn't] control[led] OBJECT`: the shared player equality of the
/// object's controller (its last known controller once it left the
/// battlefield, CR 608.2h) and the ability's controller. Only an effect
/// program's resolution binds the object.
fn lower_object_control(
    node: &ObjectControlConditionNode,
    bindings: Option<&ConditionObjectBindings<'_>>,
) -> Lowered {
    let bindings = bindings.ok_or("an object binding of an effect program")?;
    let ConditionObjectReference::Permanent(reference) = node.object else {
        return Err("a permanent whose controller the runtime reads");
    };
    let object = bound_permanent_reference_value(reference, bindings.permanent_targets)
        .ok_or("the previously declared permanent target slot in this lexical scope")?;
    let fact = json!({
        "kind": "playersEqual",
        "left": { "kind": "controllerOf", "object": object },
        "right": controller(),
    });
    Ok(if node.negated { not(fact) } else { fact })
}

/// `it's [not] their turn`: whether the active player is the player the
/// triggering event records (CR 500.1). `an opponent's turn` is the turn of
/// a player other than the controller, the negated controller turn.
fn lower_player_turn(node: &PlayerTurnNode) -> Lowered {
    let fact = match node.player {
        ConditionPlayerSubject::EventPlayer => json!({
            "kind": "isPlayersTurn",
            "player": { "kind": "triggeringPlayer" },
        }),
        ConditionPlayerSubject::Quantified {
            quantifier: PlayerQuantifier::Some,
            population: PlayerPopulation::Opponents,
        } => negation(json!({ "kind": "duringControllerTurn", "player": controller() })),
        _ => return Err("the turn of the controller, of an opponent or of the event's player"),
    };
    Ok(if node.is_turn { fact } else { negation(fact) })
}

/// A player designation for its holder: the controller, the player the
/// triggering event records, or each member of a quantified population
/// (`playerQuantifier` over `quantifiedPlayer`, CR 102.2-102.3).
fn lower_player_designation(node: &PlayerDesignationNode) -> Lowered {
    let held = |player: Value| -> Lowered {
        let atom = match &node.designation {
            PlayerDesignation::EnduringStory => {
                json!({ "kind": "hasEnduringStory", "player": player })
            }
            PlayerDesignation::CityBlessing => {
                json!({ "kind": "hasCityBlessing", "player": player })
            }
            PlayerDesignation::CompletedDungeon { dungeon } => {
                let mut atom = json!({ "kind": "completedDungeon", "player": player });
                if let Some(dungeon) = dungeon {
                    atom["name"] = json!(dungeon);
                }
                atom
            }
            PlayerDesignation::Monarch => json!({ "kind": "playerIsMonarch", "player": player }),
            PlayerDesignation::StartingPlayer => {
                json!({ "kind": "playerWasStartingPlayer", "player": player })
            }
        };
        Ok(if node.held { atom } else { not(atom) })
    };
    if !node.designation.is_per_player() && node.subject != ConditionPlayerSubject::Controller {
        return Err("a designation the runtime records for the ability's controller");
    }
    match node.subject {
        ConditionPlayerSubject::Controller => held(controller()),
        ConditionPlayerSubject::EventPlayer => held(json!({ "kind": "triggeringPlayer" })),
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
            let inner = held(json!({ "kind": "quantifiedPlayer" }))?;
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
                PlayerQuantifier::None => not(quantified("some")),
            })
        }
        ConditionPlayerSubject::DefendingPlayer | ConditionPlayerSubject::OpponentsTogether => {
            Err("a designation holder the condition lowering binds")
        }
    }
}

/// `OBJECT is [not] in ZONE [or ZONE]` (CR 400.1). The source on the
/// battlefield is the continuous evaluator's `sourceOnBattlefield`; any
/// other object or zone is the generic `objectInZone` over the referenced
/// object, resolved like every object condition subject.
fn lower_object_zone(
    node: &ObjectZoneConditionNode,
    bindings: Option<&ConditionObjectBindings<'_>>,
) -> Lowered {
    let fact = if node.object == ConditionObjectReference::SOURCE
        && node.zones == [ConditionZone::Battlefield]
    {
        json!({ "kind": "sourceOnBattlefield" })
    } else {
        let object = match (node.object, bindings) {
            (object, _) if object == ConditionObjectReference::SOURCE => self_ref(),
            (ConditionObjectReference::Permanent(reference), Some(bindings)) => {
                bound_permanent_reference_value(reference, bindings.permanent_targets)
                    .ok_or("the previously declared permanent target slot in this lexical scope")?
            }
            (ConditionObjectReference::CardTarget(binding), Some(bindings)) => bindings
                .card_targets
                .iter()
                .find_map(|(candidate, id)| (*candidate == binding).then(|| chosen_target(id)))
                .ok_or("the singular card target declared earlier in this sequence")?,
            (ConditionObjectReference::CardFocus, Some(bindings)) => bindings
                .card_focus
                .clone()
                .ok_or("a singular card result in the current card-collection scope")?,
            _ => return Err("an object whose current zone the runtime can read"),
        };
        json!({
            "kind": "objectInZone",
            "object": object,
            "zones": node.zones.iter().map(|zone| zone.ir_name()).collect::<Vec<_>>(),
        })
    };
    Ok(if node.negated { not(fact) } else { fact })
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
        CastFact::CastDuringMainPhase => {
            json!({ "kind": "wasCastDuringMainPhase", "object": self_ref() })
        }
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
fn lower_comparison(
    comparison: &ConditionComparison,
    bindings: Option<&ConditionObjectBindings<'_>>,
) -> Lowered {
    let lower_condition_number =
        |number: &ConditionNumber, player: Value| lower_condition_number(number, player, bindings);
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
        // `defending player ...` in a static combat restriction: the
        // defending player the runtime binds for each combat check.
        ConditionPlayerSubject::DefendingPlayer => {
            let player = || json!({ "kind": "defendingPlayer" });
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

fn lower_condition_number(
    number: &ConditionNumber,
    player: Value,
    bindings: Option<&ConditionObjectBindings<'_>>,
) -> Lowered {
    match number {
        ConditionNumber::Integer(value) => Ok(integer(*value)),
        // A target property (`that creature's power`) reads the permanent
        // target the program declared before the condition (CR 608.2c).
        ConditionNumber::Expression(expression) => lower_numeric_expression(expression)
            .or_else(|| {
                let targets = bindings?.permanent_targets;
                let resolve = |binding: TargetBinding| {
                    targets
                        .iter()
                        .find(|(declared, _)| *declared == binding)
                        .map(|(_, id)| json!({ "kind": "chosenTarget", "id": id }))
                };
                lower_numeric_expression_with_bound_targets(expression, &resolve)
            })
            .ok_or("a numeric expression without a target binding"),
        // Bound by the program lowering to the action before the condition.
        ConditionNumber::ActionResultObjects(reference) => lower_action_result_objects(reference),
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
        // CR 102.2-102.3: the extreme value of one measure over the
        // opponents of `player`, each opponent read as the measure's player.
        ConditionNumber::GreatestOpponent(measure) => Ok(json!({
            "kind": "opponentExtremum",
            "extremum": "greatest",
            "player": player,
            "value": lower_player_measure(measure, json!({ "kind": "quantifiedPlayer" })),
        })),
        ConditionNumber::LeastOpponent(measure) => Ok(json!({
            "kind": "opponentExtremum",
            "extremum": "least",
            "player": player,
            "value": lower_player_measure(measure, json!({ "kind": "quantifiedPlayer" })),
        })),
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
        HistoryEvent::CombatPhaseStarted => "combatPhaseStarted",
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
    compare(
        operator,
        lower_event_history_count(pattern, node.scope, node.aggregate),
        integer(node.threshold),
    )
}

/// The engine's generic event count (`countEventsThisTurn` /
/// `countEventsPreviousTurn`) of one history pattern. A comparison atom and
/// a counted numeric expression (`for each spell you've cast this turn`)
/// share it.
pub(in crate::oracle::canonical) fn lower_event_history_count(
    pattern: &EventHistoryPattern,
    scope: HistoryScope,
    aggregate: HistoryAggregate,
) -> Value {
    let mut count = json!({
        "kind": match scope {
            HistoryScope::ThisTurn | HistoryScope::ThisCombat => "countEventsThisTurn",
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
    match scope {
        HistoryScope::ThisTurn | HistoryScope::ThisCombat => {
            count["where"] = filter.unwrap_or(Value::Null);
        }
        HistoryScope::LastTurn => {
            if let Some(filter) = filter {
                count["where"] = filter;
            }
        }
    }
    if scope == HistoryScope::ThisCombat {
        // CR 506.1: the records since the current combat phase began.
        count["since"] = history_event_kind(HistoryEvent::CombatPhaseStarted);
    }
    if let Some(zone) = qualifiers.cast_zone {
        // CR 601.2a: the zone the spell-cast record keeps.
        count["fromZone"] = json!(match zone {
            SourceCastZone::Hand => "hand",
            SourceCastZone::Graveyard => "graveyard",
            SourceCastZone::Exile => "exile",
        });
    }
    let mut detail = serde_json::Map::new();
    if let Some(counter) = &qualifiers.counter {
        detail.insert("counter".to_string(), json!(counter.as_str()));
    }
    if let Some(HistoryDestination::Hand) = qualifiers.destination {
        detail.insert("destination".to_string(), json!("hand"));
    }
    if let Some(combat) = qualifiers.combat_damage {
        detail.insert("combat".to_string(), json!(combat));
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
        HistoryMeasure::Power => count["measure"] = json!("power"),
    }
    if scope == HistoryScope::LastTurn || aggregate == HistoryAggregate::GreatestPerPlayer {
        count["aggregate"] = json!(match aggregate {
            HistoryAggregate::Total => "total",
            HistoryAggregate::GreatestPerPlayer => "greatestPerPlayer",
        });
    }
    count
}

/// An object condition. Without program bindings, the only object is the
/// ability's source; its tapped, combat, modified and designation states
/// lower to the continuous evaluator's source-state kinds.
fn lower_object_condition(
    node: &ReferencedObjectConditionNode,
    bindings: Option<&ConditionObjectBindings<'_>>,
) -> Lowered {
    let Some(bindings) = bindings else {
        if let ConditionObjectReference::Permanent(PermanentEffectReference::Attached(_)) =
            node.object
        {
            // The permanent the source is attached to (CR 301.5a, 303.4a).
            let filter = lower_permanent_object_predicate(&node.predicate);
            if filter.is_null() {
                return Err("a characteristic that lowers to a nonempty object filter");
            }
            return Ok(json!({
                "kind": "objectMatchesFilter",
                "object": { "kind": "attachedPermanent", "attachment": self_ref() },
                "where": filter,
            }));
        }
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
    if let ConditionObjectReference::DepartedTarget(binding) = node.object {
        // CR 608.2h: the target's last known information. A numeric
        // property reads the target's captured values; a type-line
        // predicate reads its departure record (the current object wins
        // while it is still on the battlefield).
        let target = bound_permanent_reference_value(
            PermanentEffectReference::Target(binding),
            bindings.permanent_targets,
        )
        .ok_or("the previously declared permanent target slot in this lexical scope")?;
        if let ObjectPredicate::NumericComparison {
            left,
            operator,
            right,
        } = &node.predicate
        {
            let mut comparison = lower_selector_numeric_comparison(*left, *operator, right)
                .ok_or("a numeric bound the last-known comparison can evaluate")?;
            comparison["left"]["object"] = target;
            return Ok(comparison);
        }
        let filter = lower_permanent_object_predicate(&node.predicate);
        if filter.is_null() {
            return Err("a characteristic that lowers to a nonempty object filter");
        }
        return Ok(json!({
            "kind": "departedObjectMatches",
            "object": target,
            "where": filter,
        }));
    }
    let (object, filter) = match node.object {
        ConditionObjectReference::DepartedTarget(_) => {
            unreachable!("a departed target was lowered above")
        }
        ConditionObjectReference::Permanent(reference) => (
            bound_permanent_reference_value(reference, bindings.permanent_targets)
                .ok_or("the previously declared permanent target slot in this lexical scope")?,
            match reference {
                // The card an earlier action moved is read as that card.
                PermanentEffectReference::ActionResult(result) if !result.permanent => {
                    lower_card_object_predicate(&node.predicate)
                }
                _ => lower_permanent_object_predicate(&node.predicate),
            },
        ),
        // The spell the triggering cast record binds, on the stack.
        ConditionObjectReference::EventSpell => (
            json!({ "kind": "triggeringStackObject" }),
            lower_spell_object_predicate(&node.predicate).unwrap_or(Value::Null),
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
