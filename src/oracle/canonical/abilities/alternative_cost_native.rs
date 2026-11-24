use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

/// Claims the structural conditional free-cast envelope without constraining
/// the child controlled-permanent predicate. This lets commander, legendary
/// creature, and future typed predicates reuse the same machine.
pub(in crate::oracle::canonical) fn claims_promoted_conditional_free_cast_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("you"))
        && tokens
            .get(2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("control"))
        && tokens.windows(6).any(|window| {
            ["you", "may", "cast", "this", "spell", "without"]
                .iter()
                .zip(window)
                .all(|(expected, token)| token.raw.eq_ignore_ascii_case(expected))
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_conditional_free_cast(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("conditional-free-cast", input.tokens);
    let envelope = recognize_conditional_free_cast_envelope(&mut cursor)?;
    let CurrentSpellManaCostReferenceNode = envelope.replaces;
    cursor.require(
        envelope.costs.costs.is_empty() && envelope.costs.connectors.is_empty(),
        "the zero-cost alternative represented by an empty cost sequence",
    )?;
    let condition = cursor.require_value(envelope.condition, "a free-cast condition")?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": lower_alternative_cast_permission(None, Vec::new(), Some(lower_condition(&condition))),
        }),
        &[
            "Recognize a conditional permission to cast the current spell",
            "Resolve the controlled-permanent predicate through the shared selector",
            "Represent the omitted mana payment as a zero-cost alternative",
            "Keep additional spell costs outside the alternative mana-cost primitive",
            &trace,
        ],
    ))
}

/// Claims hand-exile alternative costs, optionally preceded by a turn
/// condition and optionally composed with a fixed life payment.
/// The narrow envelope keeps unrelated alternative-cost grammars at their
/// explicit migration boundaries while committed child failures remain
/// visible for malformed members of this family.
pub(in crate::oracle::canonical) fn claims_promoted_compound_alternative_cost_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(subject) = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("you"))
    else {
        return false;
    };
    let conditioned = tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"));
    let supported_condition = !conditioned
        || ["if", "it's", "not", "your", "turn"]
            .iter()
            .enumerate()
            .all(|(index, expected)| {
                tokens
                    .get(index)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
            });
    supported_condition
        && (conditioned || subject == 0)
        && tokens
            .get(subject + 1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("may"))
        && (tokens
            .get(subject + 2)
            .is_some_and(|token| is_verb_form(&token.raw, VerbLemma::Exile, VerbForm::Base))
            || (tokens
                .get(subject + 2)
                .is_some_and(|token| is_verb_form(&token.raw, VerbLemma::Pay, VerbForm::Base))
                && tokens
                    .get(subject + 3)
                    .is_some_and(|token| token.token_type == "quantity")
                && tokens
                    .get(subject + 4)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case("life"))))
        && tokens.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("rather")
                && window[1].raw.eq_ignore_ascii_case("than")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_compound_alternative_cost(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("alternative-mana-cost", input.tokens);
    let envelope = recognize_alternative_mana_cost_envelope(&mut cursor)?;
    let AlternativeManaCostEnvelopeNode {
        condition,
        costs,
        replaces,
    } = envelope;
    let CurrentSpellManaCostReferenceNode = replaces;
    cursor.require(
        costs.connectors.len() + 1 == costs.costs.len(),
        "one connector between each alternative cost",
    )?;
    cursor.require(
        costs
            .connectors
            .iter()
            .all(|connector| *connector == CostConnector::And),
        "`and` between composed alternative costs",
    )?;
    let lowered_costs = costs
        .costs
        .iter()
        .map(|cost| match cost {
            CompoundCostNode::PayLife(pay_life) => alternative_life_payment(pay_life.amount),
            CompoundCostNode::ExileQualifiedCardFromHand(exile) => json!({
                "kind": "exileCard",
                "zone": {
                    "kind": "hand",
                    "player": controller(),
                },
                "where": {
                    "kind": "colorContains",
                    "value": exile.color.oracle_name(),
                },
            }),
        })
        .collect::<Vec<_>>();
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let ability = lower_alternative_cast_permission(
        None,
        lowered_costs,
        condition.as_ref().map(lower_condition),
    );

    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": ability,
        }),
        &[
            "Recognize the alternative-mana-cost envelope from the shared token stream",
            "Compose independent pay-life and qualified hand-exile cost nodes",
            "Resolve the replaced cost as the current spell's mana cost",
            "Lower both costs to existing primitive IR",
            &trace,
        ],
    ))
}

/// The one alternative-cast permission (CR 118.9, 601.2b): the costs paid
/// instead of the spell's mana cost, an optional condition evaluated when
/// the spell is cast, and the mode its cast record keeps for later `if its
/// [keyword] cost was paid` checks. Every alternative-cost parent and every
/// keyword whose definition is such a permission lowers through it.
pub(in crate::oracle::canonical) fn lower_alternative_cast_permission(
    mode: Option<&str>,
    costs: Vec<Value>,
    condition: Option<Value>,
) -> Value {
    let mut ability = json!({ "kind": "alternativeCost", "costs": costs });
    if let Some(mode) = mode {
        ability["mode"] = json!(mode);
    }
    if let Some(condition) = condition {
        ability["condition"] = condition;
    }
    ability
}

fn alternative_life_payment(amount: i64) -> Value {
    json!({ "kind": "payLife", "player": controller(), "amount": integer(amount) })
}

/// One printed payment of an alternative cost, in the schema the casting
/// runtime declares and pays (CR 601.2h). A payment that schema cannot
/// carry fails explicitly.
fn lower_alternative_cast_payment(
    cursor: &OracleTokenCursor<'_>,
    cost: &LocalActivationCostNode,
) -> MachineResult<Value> {
    let chosen = |exclude_source: bool| {
        cursor.require(
            !exclude_source,
            "an alternative-cost object other than the spell (`another` has no alternative-cost runtime)",
        )
    };
    Ok(match cost {
        LocalActivationCostNode::PayMana(mana) => {
            mana.fixed_runtime_mana_value()
                .map_err(|error| cursor.fail(error.expected()))?;
            lower_resource_mana_payment(mana)
        }
        LocalActivationCostNode::PayLife(life) => alternative_life_payment(life.amount),
        LocalActivationCostNode::SacrificeObject(sacrifice) => {
            chosen(sacrifice.exclude_source)?;
            json!({
                "kind": "sacrificePermanent",
                "where": lower_permanent_object_predicate(&sacrifice.object.predicate),
                "count": integer(sacrifice.count),
            })
        }
        LocalActivationCostNode::TapObjects(tap) => {
            chosen(tap.tap.exclude_source)?;
            json!({
                "kind": "tap",
                "where": lower_permanent_object_predicate(&tap.tap.object.predicate),
                "count": integer(tap.tap.count),
            })
        }
        LocalActivationCostNode::DiscardCard(discard)
            if discard.method == CollectionSelectionMethod::Choice =>
        {
            json!({
                "kind": "discardCard",
                "where": lower_card_object_predicate(&discard.filter),
                "count": integer(discard.count),
            })
        }
        LocalActivationCostNode::ReturnObjects(returned) if returned.count == 1 => {
            chosen(returned.exclude_source)?;
            alternative_return_payment(&returned.object.predicate)
        }
        LocalActivationCostNode::ExileCards(exile)
            if exile.zone == PaymentOriginZone::Hand && exile.count == 1 =>
        {
            chosen(exile.exclude_source)?;
            json!({
                "kind": "exileCard",
                "zone": { "kind": "hand", "player": controller() },
                "where": lower_card_object_predicate(&exile.filter),
            })
        }
        _ => {
            return Err(cursor.fail(
                "an alternative-cost payment the casting runtime pays: mana, life, chosen permanents to sacrifice or tap, chosen cards to discard, or one permanent to return or hand card to exile",
            ));
        }
    })
}

/// `[If COND,] you may COSTS rather than pay this spell's mana cost [if
/// COND].` and `... you may cast this spell without paying its mana cost.`
pub(in crate::oracle::canonical) fn claims_alternative_cast_sentence_family(
    input: &AbilityInput<'_>,
) -> bool {
    alternative_cast_sentence_starts(input.tokens)
}

pub(in crate::oracle::canonical) fn parse_alternative_cast_sentence(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("alternative-cast", input.tokens);
    let context = ConditionContext::standalone(
        ConditionScope::Static,
        input.name_links,
        input.name_resolution_failure,
        false,
    );
    let sentence = recognize_alternative_cast_sentence(&mut cursor, &context)?;
    cursor.enter("alternative-cast.lowering");
    let costs = match &sentence.costs {
        Some(costs) => costs
            .costs
            .iter()
            .map(|cost| lower_alternative_cast_payment(&cursor, cost))
            .collect::<MachineResult<Vec<_>>>()?,
        None => Vec::new(),
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": lower_alternative_cast_permission(
                None,
                costs,
                sentence.condition.as_ref().map(lower_condition),
            ),
        }),
        &[
            "Recognize the alternative-cast permission and its optional condition",
            "Read the payments with the shared activation-cost sequence",
            "Read the condition with the shared condition atom program",
            "Lower through the one alternative-cast permission",
            &trace,
        ],
    ))
}

/// A controlled permanent returned to its owner's hand as an alternative
/// cost (CR 118.9); the runtime chooses it among the payer's permanents.
pub(in crate::oracle::canonical) fn alternative_return_payment(
    predicate: &ObjectPredicate,
) -> Value {
    json!({
        "kind": "returnPermanentToOwnersHand",
        "where": lower_permanent_object_predicate(predicate),
    })
}

/// A controlled permanent sacrificed as an alternative cost (CR 118.9);
/// the runtime declares it among the payer's permanents.
pub(in crate::oracle::canonical) fn alternative_sacrifice_payment(
    predicate: &ObjectPredicate,
) -> Value {
    json!({
        "kind": "sacrificePermanent",
        "where": lower_permanent_object_predicate(predicate),
    })
}

// ---------------------------------------------------------------------------
// Casting keywords (CR 702)

pub(in crate::oracle::canonical) fn claims_casting_keyword_family(
    input: &AbilityInput<'_>,
) -> bool {
    casting_keyword_starts(input.tokens).is_some()
}

fn keyword_mana(
    cursor: &OracleTokenCursor<'_>,
    node: &CastingKeywordNode,
    expected: &str,
) -> MachineResult<Value> {
    node.cost
        .as_ref()
        .and_then(KeywordCostNode::single_mana)
        .map(lower_resource_mana_payment)
        .ok_or_else(|| cursor.fail(expected))
}

/// One keyword payment: the printed mana cost, or the only payment of the
/// shared cost sequence through the keyword-cost lowering.
fn single_keyword_payment(
    cursor: &OracleTokenCursor<'_>,
    node: &CastingKeywordNode,
    expected: &str,
) -> MachineResult<Value> {
    match node.cost.as_ref() {
        Some(KeywordCostNode::Mana(mana)) => Ok(lower_resource_mana_payment(mana)),
        Some(KeywordCostNode::Sequence(sequence)) => match sequence.costs.as_slice() {
            [payment] => lower_keyword_cost_payment(cursor, payment),
            _ => Err(cursor.fail(expected)),
        },
        None => Err(cursor.fail(expected)),
    }
}

fn keyword_count(cursor: &OracleTokenCursor<'_>, node: &CastingKeywordNode) -> MachineResult<i64> {
    cursor.require_value(node.count, "the keyword's printed count")
}

/// A condition of a keyword definition, read by the shared condition atom
/// grammar from the definition's own CR words, as a state with no program
/// bindings (it is evaluated as the spell is cast).
fn definition_condition(words: &str) -> MachineResult<Value> {
    let tokens = crate::oracle::syntax::recognize_entities(words);
    let mut cursor = OracleTokenCursor::new("casting-keyword-definition", &tokens);
    let atom = recognize_condition_atom(
        &mut cursor,
        &ConditionContext::standalone(ConditionScope::Static, &[], None, false),
    )?;
    cursor.finish()?;
    Ok(lower_condition(&BooleanConditionNode::Atom(atom)))
}

/// `you or a teammate has cast another spell this turn` (CR 702.117a): the
/// event-history atom over this turn's cast records of the controller. The
/// spell being cast is not cast yet when its cost is chosen, so one earlier
/// record is `another spell`.
fn controller_cast_another_spell_this_turn() -> Value {
    lower_condition(&BooleanConditionNode::Atom(ConditionAtom::EventHistory(
        EventHistoryNode {
            pattern: EventHistoryPattern {
                event: HistoryEvent::SpellCast,
                player: HistoryPlayer::Controller,
                object: None,
                measure: HistoryMeasure::Occurrences,
                qualifiers: HistoryQualifiers::default(),
            },
            scope: HistoryScope::ThisTurn,
            aggregate: HistoryAggregate::Total,
            operator: NumericComparisonOperator::GreaterThanOrEqual,
            threshold: 1,
        },
    )))
}

/// Escape (CR 702.138a): `{mana}, Exile N other cards from your graveyard`.
fn escape_ability(
    cursor: &OracleTokenCursor<'_>,
    node: &CastingKeywordNode,
) -> MachineResult<Value> {
    let Some(KeywordCostNode::Sequence(sequence)) = node.cost.as_ref() else {
        return Err(cursor.fail("an escape cost sequence after its em dash"));
    };
    let [
        LocalActivationCostNode::PayMana(mana),
        LocalActivationCostNode::ExileCards(exile),
    ] = sequence.costs.as_slice()
    else {
        return Err(cursor.fail(
            "an escape cost of one mana payment and other cards exiled from your graveyard",
        ));
    };
    cursor.require(
        exile.zone == PaymentOriginZone::Graveyard && exile.exclude_source && exile.count > 0,
        "other cards exiled from the caster's graveyard (CR 702.138a)",
    )?;
    Ok(json!({
        "kind": "escape",
        "costs": [
            lower_resource_mana_payment(mana),
            {
                "kind": "exileGraveyardCards",
                "player": controller(),
                "where": lower_card_object_predicate(&exile.filter),
                "excludeSource": true,
                "quantity": { "kind": "exactly", "count": integer(exile.count) },
                "decisionId": "escapeExileCards",
            },
        ],
    }))
}

/// The rules definition of one casting keyword, as the runtime executes it.
fn lower_casting_keyword(
    cursor: &OracleTokenCursor<'_>,
    node: &CastingKeywordNode,
    face_name: &str,
    face_type_line: &str,
) -> MachineResult<Value> {
    Ok(match node.keyword {
        // CR 702.137a: pay [cost] rather than the mana cost if an opponent
        // lost life this turn.
        CastingKeyword::Spectacle => lower_alternative_cast_permission(
            Some("spectacle"),
            vec![keyword_mana(cursor, node, "a spectacle mana cost")?],
            Some(definition_condition("an opponent lost life this turn")?),
        ),
        // CR 702.117a.
        CastingKeyword::Surge => lower_alternative_cast_permission(
            Some("surge"),
            vec![keyword_mana(cursor, node, "a surge mana cost")?],
            Some(controller_cast_another_spell_this_turn()),
        ),
        // CR 702.188a: pay [cost] and return a tapped creature you control
        // to its owner's hand rather than pay the mana cost.
        CastingKeyword::WebSlinging => lower_alternative_cast_permission(
            Some("webSlinging"),
            vec![
                keyword_mana(cursor, node, "a web-slinging mana cost")?,
                alternative_return_payment(&ObjectPredicate::Intersection(vec![
                    ObjectPredicate::CardType(ObjectCardType::Creature),
                    ObjectPredicate::Status(ObjectStatus::Tapped),
                ])),
            ],
            None,
        ),
        // CR 702.119a: pay [cost] and sacrifice a creature rather than the
        // mana cost; the total cost is reduced by its mana value.
        CastingKeyword::Emerge => {
            let mut sacrifice =
                alternative_sacrifice_payment(&ObjectPredicate::CardType(ObjectCardType::Creature));
            sacrifice["reduceByManaValue"] = json!(true);
            lower_alternative_cast_permission(
                Some("emerge"),
                vec![
                    keyword_mana(cursor, node, "an emerge mana cost")?,
                    sacrifice,
                ],
                None,
            )
        }
        // CR 702.190a: pay [cost] and return an unblocked creature you
        // control rather than the mana cost, any time an instant could be
        // cast during your declare blockers step.
        CastingKeyword::Sneak => {
            cursor.require(
                face_type_line
                    .split(|character: char| !character.is_alphanumeric())
                    .any(|word| word.eq_ignore_ascii_case("instant")),
                "an instant face for sneak (a sneak permanent or sorcery spell needs the declare-blockers cast timing and the attacking entry of CR 702.190b, which the casting runtime lacks)",
            )?;
            lower_alternative_cast_permission(
                Some("sneak"),
                vec![
                    keyword_mana(cursor, node, "a sneak mana cost")?,
                    alternative_return_payment(&ObjectPredicate::Intersection(vec![
                        ObjectPredicate::CardType(ObjectCardType::Creature),
                        ObjectPredicate::Status(ObjectStatus::Unblocked),
                    ])),
                ],
                Some(json!({
                    "kind": "and",
                    "operands": [
                        { "kind": "duringControllerTurn", "player": controller() },
                        { "kind": "duringSteps", "steps": ["declareBlockers"] },
                    ],
                })),
            )
        }
        CastingKeyword::Escape => escape_ability(cursor, node)?,
        // The runtime owns blitz's haste, dies-draw and delayed sacrifice
        // (CR 702.152a); it pays a mana blitz cost only.
        CastingKeyword::Blitz => json!({
            "kind": "blitz",
            "cost": keyword_mana(
                cursor,
                node,
                "a mana blitz cost (no other blitz payment has a runtime)",
            )?,
        }),
        // CR 702.187b: the graveyard route admits a card discarded this turn.
        CastingKeyword::Mayhem => json!({
            "kind": "mayhem",
            "cost": keyword_mana(cursor, node, "a mayhem mana cost")?,
        }),
        // CR 702.120a: the runtime pays the cost once per extra mode, with a
        // declared object for each nonmana payment.
        CastingKeyword::Escalate => json!({
            "kind": "escalate",
            "cost": single_keyword_payment(cursor, node, "one escalate payment")?,
        }),
        // CR 702.42a: choosing every mode adds the entwine mana cost.
        CastingKeyword::Entwine => json!({
            "kind": "entwine",
            "cost": keyword_mana(
                cursor,
                node,
                "a mana entwine cost (a nonmana entwine payment has no runtime)",
            )?,
        }),
        // CR 702.33a: a kicked spell also pays the nonmana kicker cost.
        CastingKeyword::Kicker => json!({
            "kind": "kicker",
            "cost": single_keyword_payment(cursor, node, "one nonmana kicker payment")?,
            "repeatable": false,
        }),
        CastingKeyword::Impending => json!({
            "kind": "impending",
            "timeCounters": integer(keyword_count(cursor, node)?),
            "cost": keyword_mana(cursor, node, "an impending mana cost")?,
        }),
        CastingKeyword::Awaken => json!({
            "kind": "awaken",
            "count": integer(keyword_count(cursor, node)?),
            "cost": keyword_mana(cursor, node, "an awaken mana cost")?,
        }),
        CastingKeyword::Prototype => {
            let (power, toughness) =
                cursor.require_value(node.power_toughness, "the prototype power/toughness")?;
            json!({
                "kind": "prototype",
                "cost": keyword_mana(cursor, node, "a prototype mana cost")?,
                "power": integer(power),
                "toughness": integer(toughness),
            })
        }
        CastingKeyword::Teamwork => json!({
            "kind": "teamwork",
            "minimumPower": integer(keyword_count(cursor, node)?),
        }),
        CastingKeyword::Conspire => json!({ "kind": "conspire" }),
        // CR 702.81a: cast from the graveyard by also discarding a land card.
        CastingKeyword::Retrace => json!({
            "kind": "graveyardCastWithDiscard",
            "keyword": "retrace",
            "where": lower_card_object_predicate(&ObjectPredicate::CardType(ObjectCardType::Land)),
        }),
        // CR 702.133a: cast from the graveyard by also discarding a card;
        // the spell is exiled when it leaves the stack.
        CastingKeyword::JumpStart => {
            cursor.require(
                spell_face(face_type_line),
                "an instant or sorcery face for jump-start",
            )?;
            json!({
                "kind": "graveyardCastWithDiscard",
                "keyword": "jumpStart",
                "exileOnLeaveStack": true,
            })
        }
        CastingKeyword::Gravestorm => json!({ "kind": "gravestorm" }),
        // CR 702.174a: an optional promise as the spell is cast; the
        // runtime has the chosen opponent draw a card first.
        CastingKeyword::GiftACard => json!({ "kind": "giftCard", "optional": true }),
        // CR 702.192a: the spell's own name identifies its later copies.
        CastingKeyword::Paradigm => {
            cursor.require(!face_name.trim().is_empty(), "the paradigm spell's name")?;
            json!({ "kind": "paradigm", "spellName": face_name })
        }
    })
}

pub(in crate::oracle::canonical) fn parse_casting_keyword(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("casting-keyword", input.tokens);
    let node = recognize_casting_keyword(&mut cursor)?;
    cursor.enter("keyword.casting.lowering");
    let ability = lower_casting_keyword(&cursor, &node, input.face_name, input.face_type_line)?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    let expansion = format!(
        "Expand {} (CR {}) with its printed parameter into its casting rule",
        node.keyword.lexeme(),
        node.keyword.rules_section()
    );
    let reminder = if node.reminder {
        "Validate the printed reminder against the keyword's registered renderings"
    } else {
        "No printed reminder"
    };
    Ok(draft(
        json!({ "kind": "keywordAbility", "source": self_ref(), "ability": ability }),
        &[
            "Recognize a casting keyword lexeme and its parameter from the shared token stream",
            "Read the parameter with the shared count, mana-cost or cost-sequence child",
            reminder,
            &expansion,
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "alternative_cast_sentence_tests.rs"]
mod alternative_cast_sentence_tests;
