use super::super::{MachineResult, OracleTokenCursor, parse_number_word, parse_ordinal_word};
use super::arithmetic_expression::recognize_arithmetic_expression;
use super::{
    EventNumericBinding, NumericAtom, NumericExpression, TargetBinding,
    recognize_source_object_property,
};

/// Resolves Oracle's `that many`/`that much` numeric anaphor. An explicit
/// parent binding wins; otherwise the binding the enclosing triggered ability
/// installed on the cursor applies; otherwise the nearest earlier action of
/// the enclosing program is the antecedent. Without any it is a failure.
pub(in crate::oracle::canonical) fn recognize_that_many_event_value(
    cursor: &mut OracleTokenCursor<'_>,
    binding: Option<EventNumericBinding>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.event-anaphor.demonstrative");
    cursor.consume("that")?;
    cursor.enter("numeric.event-anaphor.quantity");
    let quantity = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("many") || raw.eq_ignore_ascii_case("much"))
        .ok_or_else(|| cursor.fail("many or much"))?
        .to_string();
    cursor.consume(&quantity)?;
    cursor.enter("numeric.event-anaphor.resolve");
    match binding.or(cursor.event_numeric_binding()) {
        Some(binding) => Ok(NumericExpression::EventValue(binding)),
        // Without an event quantity the antecedent is the result of an
        // earlier action of the same program (CR 608.2c); the program
        // lowering binds it or fails explicitly.
        // The first action of an activation restates its payer-chosen
        // cost number (`Remove any number of counters: ... that many`).
        None => match super::cost_chosen_amount_anaphor(cursor) {
            Some(amount) => Ok(amount),
            None => super::action_result_anaphor(cursor, quantity.eq_ignore_ascii_case("much")),
        },
    }
}

/// `the number of card types among COLLECTION`: the shared `the number of`
/// aggregate whose measure is card types. A parent that needs exactly this
/// measure (a characteristic-defining P/T) type-checks the aggregate it
/// read; the phrase itself is no separate grammar.
pub(in crate::oracle::canonical) fn recognize_distinct_card_type_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    let count = super::recognize_number_of_count(cursor)?;
    cursor.enter("numeric.distinct-card-types.measure");
    cursor.require(
        matches!(count, NumericExpression::CountDistinctCardTypes { .. }),
        "the number of card types among a graveyard collection",
    )?;
    Ok(count)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericState {
    Start,
    Literal,
    Variable,
    Complete,
}

/// Recognizes one atomic numeric expression without rebuilding source text.
/// Compound arithmetic can call this machine for each operand later.
pub(in crate::oracle::canonical) fn recognize_numeric_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericAtom> {
    let mut state = NumericState::Start;
    let mut value = None;
    loop {
        state = match state {
            NumericState::Start => {
                cursor.enter("numeric.atom.start");
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("integer, number word, or X"))?;
                if token.raw.eq_ignore_ascii_case("x") {
                    NumericState::Variable
                } else if parse_number_word(&token.raw).is_some() {
                    NumericState::Literal
                } else {
                    return Err(cursor.fail("integer, number word, or X"));
                }
            }
            NumericState::Literal => {
                cursor.enter("numeric.atom.literal");
                // Articles keep their lexical determiner role. In a numeric
                // expression this child resolves their contextual value to
                // one; it must not force the dictionary to classify all `a`
                // tokens as quantities before their parent is known.
                let article = cursor.peek_token().is_some_and(|token| {
                    token.token_type == "quantifierWord"
                        && (token.raw.eq_ignore_ascii_case("a")
                            || token.raw.eq_ignore_ascii_case("an"))
                });
                let token = if article {
                    cursor.consume_type("quantifierWord")?
                } else {
                    cursor.consume_type("quantity")?
                };
                value = Some(NumericAtom::Integer(
                    parse_number_word(&token.raw)
                        .ok_or_else(|| cursor.fail("a supported literal quantity"))?,
                ));
                NumericState::Complete
            }
            NumericState::Variable => {
                cursor.enter("numeric.atom.variable");
                cursor.consume("x")?;
                value = Some(NumericAtom::VariableX);
                NumericState::Complete
            }
            NumericState::Complete => {
                cursor.enter("numeric.atom.complete");
                return cursor.require_value(value, "a populated numeric atom");
            }
        };
    }
}

/// Recognizes one ordinal literal as a reusable numeric child. Sequence
/// constraints (for example, consecutive turns) remain the responsibility of
/// the parent that owns the surrounding list.
pub(in crate::oracle::canonical) fn recognize_ordinal_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<i64> {
    cursor.enter("numeric.ordinal.literal");
    let token = cursor
        .peek_token()
        .ok_or_else(|| cursor.fail("an ordinal literal"))?;
    let value = parse_ordinal_word(&token.raw).ok_or_else(|| cursor.fail("an ordinal literal"))?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericExpressionState {
    Start,
    Equality,
    ObjectReference,
    Property,
    Complete,
}

/// Named anaphora retain their required nominal type until the parent can
/// validate it against the original target slot. Lexical recognition alone
/// does not establish that a target is an artifact, creature, or permanent.
fn recognize_numeric_object_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<(TargetBinding, Option<super::ObjectPredicate>)> {
    cursor.enter("numeric.expression.object-reference");
    let reference = super::recognize_typed_target_reference(
        cursor,
        antecedent,
        "numeric.expression.object-reference.nominal",
    )?;
    Ok((
        reference.binding,
        reference
            .required_nominal
            .map(super::TargetNominal::as_predicate),
    ))
}

/// Recognizes one leaf of the shared arithmetic grammar without reconstructing
/// text. Object anaphora must resolve through parent memory.
pub(in crate::oracle::canonical) fn recognize_numeric_primary_expression(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<NumericExpression> {
    let mut state = NumericExpressionState::Start;
    let mut object = None;
    let mut expression = None;
    loop {
        state = match state {
            NumericExpressionState::Start => {
                cursor.enter("numeric.expression.start");
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("this"))
                {
                    expression = Some(recognize_source_object_property(cursor)?);
                    NumericExpressionState::Complete
                } else if super::source_name_possessive_property_starts(cursor) {
                    // `Skanos's power`: the source by its own name.
                    expression = Some(super::recognize_source_name_possessive_property(cursor)?);
                    NumericExpressionState::Complete
                } else if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("equal"))
                {
                    NumericExpressionState::Equality
                } else {
                    expression = Some(NumericExpression::Atom(recognize_numeric_atom(cursor)?));
                    NumericExpressionState::Complete
                }
            }
            NumericExpressionState::Equality => {
                cursor.enter("numeric.expression.equality");
                cursor.consume_phrase(&["equal", "to"])?;
                if super::equal_to_quantity_family_starts(cursor) {
                    // A count, aggregate or cost-object quantity is the
                    // same `equal to` amount every action consumer reads.
                    expression = Some(super::recognize_equal_to_amount_expression(cursor, None)?);
                    NumericExpressionState::Complete
                } else if super::source_name_possessive_property_starts(cursor) {
                    // `equal to Skanos's power`: the same source-name
                    // property the bare primary reads.
                    expression = Some(super::recognize_source_name_possessive_property(cursor)?);
                    NumericExpressionState::Complete
                } else {
                    NumericExpressionState::ObjectReference
                }
            }
            NumericExpressionState::ObjectReference => {
                // Without a parent-supplied target, `that NOUN's PROPERTY`
                // is the genitive the binding environment resolves.
                if antecedent.is_none() && super::bound_object_property_starts(cursor) {
                    let environment = super::installed_binding_environment(cursor);
                    expression = Some(super::recognize_bound_object_property(
                        cursor,
                        &environment,
                    )?);
                    NumericExpressionState::Complete
                } else if antecedent.is_none()
                    && super::environment_possessive_property_starts(cursor)
                {
                    // `its power` without a parent target: the binding
                    // environment's possessive antecedent.
                    let environment = super::installed_binding_environment(cursor);
                    expression = Some(super::recognize_environment_possessive_property(
                        cursor,
                        &environment,
                    )?);
                    NumericExpressionState::Complete
                } else {
                    object = Some(recognize_numeric_object_reference(cursor, antecedent)?);
                    NumericExpressionState::Property
                }
            }
            NumericExpressionState::Property => {
                cursor.enter("numeric.expression.property");
                let (binding, nominal) = cursor
                    .require_value(object.take(), "a resolved object for the numeric property")?;
                let property;
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("power"))
                {
                    cursor.consume("power")?;
                    property = super::SelectorNumericProperty::Power;
                } else if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("toughness"))
                {
                    cursor.consume("toughness")?;
                    property = super::SelectorNumericProperty::Toughness;
                } else {
                    cursor.consume("mana")?;
                    cursor.enter("numeric.expression.mana-value");
                    cursor.consume("value")?;
                    property = super::SelectorNumericProperty::ManaValue;
                }
                expression = Some(match nominal {
                    Some(required_nominal) => NumericExpression::QualifiedTargetProperty {
                        object: binding,
                        required_nominal,
                        property,
                    },
                    None => match property {
                        super::SelectorNumericProperty::Power => {
                            NumericExpression::PowerOf(binding)
                        }
                        super::SelectorNumericProperty::ManaValue => {
                            NumericExpression::ManaValueOf(binding)
                        }
                        // Keep the new property typed even without an explicit noun.
                        super::SelectorNumericProperty::Toughness => {
                            NumericExpression::QualifiedTargetProperty {
                                object: binding,
                                required_nominal: super::ObjectPredicate::ObjectKind(
                                    super::ObjectKind::Permanent,
                                ),
                                property,
                            }
                        }
                    },
                });
                NumericExpressionState::Complete
            }
            NumericExpressionState::Complete => {
                cursor.enter("numeric.expression.complete");
                return cursor.require_value(expression, "a populated numeric expression");
            }
        };
    }
}

/// Recognizes one numeric expression through the shared precedence-aware
/// arithmetic machine. Its primary leaves retain the existing typed numeric
/// atoms and object-property references.
pub(in crate::oracle::canonical) fn recognize_numeric_expression(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<NumericExpression> {
    recognize_arithmetic_expression(cursor, antecedent)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceCastXExpressionState {
    Start,
    Half,
    OptionalOf,
    Variable,
    Complete,
}

/// Recognizes the value of X retained from casting the current source, either
/// directly or through Oracle's `half [of] X` arithmetic prefix. The parent
/// owns the following noun and any explicit `rounded down` confirmation.
pub(in crate::oracle::canonical) fn recognize_source_cast_x_numeric_expression(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    let mut state = SourceCastXExpressionState::Start;
    let mut halved = false;
    loop {
        state = match state {
            SourceCastXExpressionState::Start => {
                cursor.enter("numeric.source-cast-x.start");
                match cursor.peek() {
                    Some(raw) if raw.eq_ignore_ascii_case("half") => {
                        SourceCastXExpressionState::Half
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("x") => {
                        SourceCastXExpressionState::Variable
                    }
                    _ => return Err(cursor.fail("X or `half X` from the source's cast value")),
                }
            }
            SourceCastXExpressionState::Half => {
                cursor.enter("numeric.source-cast-x.half");
                let token = cursor.consume_type("numericOperatorWord")?;
                cursor.require(
                    token.raw.eq_ignore_ascii_case("half"),
                    "the `half` operator",
                )?;
                halved = true;
                SourceCastXExpressionState::OptionalOf
            }
            SourceCastXExpressionState::OptionalOf => {
                cursor.enter("numeric.source-cast-x.optional-of");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
                {
                    cursor.consume("of")?;
                }
                SourceCastXExpressionState::Variable
            }
            SourceCastXExpressionState::Variable => {
                cursor.enter("numeric.source-cast-x.variable");
                let token_index = cursor.position();
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("the source cast X value"))?;
                if token.token_type != "quantity" || !token.raw.eq_ignore_ascii_case("x") {
                    return Err(cursor.fail_with_span(
                        "the source cast X value",
                        token_index,
                        token.span_start,
                        token.span_end,
                        token.raw.clone(),
                    ));
                }
                cursor.advance_to(token_index + 1)?;
                SourceCastXExpressionState::Complete
            }
            SourceCastXExpressionState::Complete => {
                cursor.enter("numeric.source-cast-x.complete");
                let expression = NumericExpression::SourceCastX;
                return if halved {
                    NumericExpression::divide_rounded_down(expression, 2)
                        .ok_or_else(|| cursor.fail("a positive literal divisor"))
                } else {
                    Ok(expression)
                };
            }
        };
    }
}

/// A typed quantity read from a game record or an aggregate of a collection
/// rather than from one object's characteristics (CR 107.1). Each variant is
/// one reusable atom: every quantity, comparison and `where X is` slot reads
/// it through the shared arithmetic grammar, and its lowering names one
/// engine evaluator or fails explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum NumericQuantity {
    /// `the amount of [COLOR] mana spent to cast SPELL` (CR 601.2h): the
    /// spell's cast record, or a permanent's record of its own cast.
    ManaSpentToCast {
        spell: super::CastSpellReference,
        symbol: Option<&'static str>,
    },
    /// `time(s) you've cast your commander from the command zone this game`
    /// (CR 903.8): the casts the commander record keeps.
    CommanderCastsFromCommandZone,
    /// Counters on a holder (`for each counter on Kyler`, `the number of
    /// +1/+1 counters on lands you control`, CR 122.1): one kind or every
    /// kind.
    Counters {
        counter: Option<crate::oracle::counter_name::CounterName>,
        holder: CounterHolder,
    },
    /// The number of different values of one characteristic among a card
    /// collection (`different mana value among nonland cards in your
    /// graveyard`, CR 202.3).
    DistinctCardValues {
        measure: DistinctCardMeasure,
        collection: AggregateCollection,
    },
    /// The mana symbols of one color among the mana costs of a card
    /// collection (`black mana symbols in the mana costs of cards in your
    /// graveyard`, CR 202.1): a hybrid or Phyrexian symbol of that color
    /// counts once.
    ManaSymbolsAmongCards {
        color: crate::oracle::characteristic_vocabulary::CardColor,
        collection: AggregateCollection,
    },
    /// The greater of several quantities (`A or B, whichever is greater`).
    Greatest(Vec<NumericExpression>),
    /// The cards in one zone of the player who performs the enclosing
    /// action (`mills half their library`, CR 401.1): the action's lowering
    /// supplies that single player, or the quantity is refused.
    ActingPlayerCards { zone: super::LocalZone },
    /// The members of a restricted player population (`the number of
    /// opponents who control an artifact`, `for each player who lost life
    /// this turn`, CR 102.1-102.3), read by the shared player-set grammar.
    Players(super::PlayerPopulationQueryNode),
    /// How many times the ability being resolved has resolved, or has been
    /// activated, during this turn (`this is the third time this ability has
    /// resolved this turn`, `this ability has been activated four or more
    /// times this turn`). The count includes the current resolution or
    /// activation; only an effect-program position reads it.
    AbilityHistory(AbilityHistoryMeasure),
    /// The sum of one numeric characteristic over a collection (`the total
    /// power of creatures you control`, `the total mana value of permanents
    /// destroyed this way`, CR 107.1): zero over an empty collection.
    PropertyTotal(PropertyTotalNode),
    /// The members of a player population whose measure compares with the
    /// ability controller's (`the number of opponents who control more
    /// lands than you`, `for each opponent who controls fewer creatures
    /// than you`, CR 102.1-102.3).
    PlayersComparedWithController {
        players: super::PlayerSetReferenceNode,
        measure: super::PlayerStateMeasure,
        operator: super::NumericComparisonOperator,
    },
    /// The number of players whose measure is the extremum of every
    /// player's (`a player has more life than each other player` holds
    /// when it is one).
    PlayersAtExtremum {
        measure: super::PlayerStateMeasure,
        extremum: super::PlayerExtremum,
    },
    /// One measure of the ability's controller (`the difference` reads the
    /// cards in your hand of the comparison it restates).
    ControllerMeasure(super::PlayerStateMeasure),
    /// `your highest commander tax among your commanders` (CR 903.8): the
    /// greatest tax among the controller's commanders, zero without one.
    HighestCommanderTax,
    /// The number the payer chose while paying a cost of this activation
    /// (`for each storage counter removed this way`, CR 601.2h via 602.2b):
    /// the activation's `costChosenAmount` declaration (`quantity_slot.rs`).
    CostChosenAmount,
    /// One function of one property over the objects of a set (`the
    /// greatest power among creatures target opponent controls`, `the number
    /// of creature types among cards in your graveyard`, `for each of its
    /// colors`, CR 107.1): the shared aggregate over any object set.
    Aggregate(ObjectAggregateNode),
    /// `the difference between A and B` (CR 107.1): the absolute value of
    /// `A - B`, never negative.
    Difference(NumericExpression, NumericExpression),
}

/// `FUNCTION PROPERTY among|of OBJECTS` (CR 107.1, 202.3, 105.2, 205.2a,
/// 205.3, 122.1): one aggregate function of one property over an object
/// set. Every reader of a set (`among`, `of`, `for each ... among`, `its`)
/// produces this one node; the set is any object set the shared readers
/// name, so a new set or property needs no new numeric grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectAggregateNode {
    pub function: AggregateFunction,
    pub property: AggregateProperty,
    pub objects: AggregateObjects,
    /// The value is undefined, not zero, over an empty set: a comparison
    /// that reads it is false (`you control the creature with the greatest
    /// power` holds only when you control a creature).
    pub require_members: bool,
}

/// How the values of the members are combined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AggregateFunction {
    /// The greatest value (`the greatest|highest PROPERTY among`); zero over
    /// an empty set.
    Greatest,
    /// The least value (`the least|lowest PROPERTY among`); zero over an
    /// empty set.
    Least,
    /// The sum of the values (`the total PROPERTY of`); zero over an empty
    /// set.
    Total,
    /// The number of different values among the members (`the number of
    /// colors among`, `for each kind of counter on`); a member with several
    /// values (a multicolored permanent) contributes each of them.
    Distinct,
}

impl AggregateFunction {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Greatest => "greatest",
            Self::Least => "least",
            Self::Total => "total",
            Self::Distinct => "distinct",
        }
    }
}

/// The property an aggregate reads on each member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AggregateProperty {
    /// CR 208.1.
    Power,
    /// CR 208.1.
    Toughness,
    /// CR 202.3.
    ManaValue,
    /// CR 105.2: an object's colors (colorless is no color).
    Colors,
    /// CR 205.2a: the card types; subtypes and supertypes are not card types.
    CardTypes,
    /// CR 205.3m: the creature types (a changeling has every one).
    CreatureTypes,
    /// CR 205.3i, 305.6: the basic land types.
    BasicLandTypes,
    /// CR 122.1: the kinds of counters on an object.
    CounterKinds,
    /// CR 201.2: the names.
    Names,
}

impl AggregateProperty {
    /// Whether the property is one number per member, which every function
    /// reads; the other properties are value sets only `Distinct` counts.
    pub(in crate::oracle::canonical) const fn is_numeric(self) -> bool {
        matches!(self, Self::Power | Self::Toughness | Self::ManaValue)
    }

    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Power => "power",
            Self::Toughness => "toughness",
            Self::ManaValue => "manaValue",
            Self::Colors => "colors",
            Self::CardTypes => "cardTypes",
            Self::CreatureTypes => "creatureTypes",
            Self::BasicLandTypes => "basicLandTypes",
            Self::CounterKinds => "counterKinds",
            Self::Names => "names",
        }
    }

    pub(in crate::oracle::canonical) const fn from_selector(
        property: super::SelectorNumericProperty,
    ) -> Self {
        match property {
            super::SelectorNumericProperty::Power => Self::Power,
            super::SelectorNumericProperty::Toughness => Self::Toughness,
            super::SelectorNumericProperty::ManaValue => Self::ManaValue,
        }
    }
}

/// The objects an aggregate reads, each read by a shared reader: a
/// battlefield collection of the shared object query (any player relation,
/// `other`), the cards of one zone of the shared scoped query, a union of
/// such sets, one object a reference names, the spell a cast event records,
/// or the objects an earlier action of the program affected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AggregateObjects {
    Permanents(super::ObjectQueryNode),
    Cards(Box<super::ScopedObjectQueryNode>),
    Union(Vec<AggregateObjects>),
    Object(super::PermanentEffectReference),
    /// `that spell`: the spell the cast event observed (CR 601.2i).
    EventSpell,
    /// `cards discarded this way`: the objects the nearest earlier action of
    /// the restated verb affected, bound by the program lowering (CR 608.2c).
    ActionResult {
        reader: usize,
        reference: super::ActionResultReferenceNode,
    },
}

impl AggregateObjects {
    fn uses_x(&self) -> bool {
        match self {
            Self::Permanents(query) => super::object_predicate_uses_x(&query.predicate),
            Self::Cards(cards) => cards.uses_x(),
            Self::Union(parts) => parts.iter().any(Self::uses_x),
            Self::Object(_) | Self::EventSpell | Self::ActionResult { .. } => false,
        }
    }

    /// Whether the set names a declared target slot of the reading action
    /// (`its colors` of a target). A player relation to a declared player
    /// (`creatures target opponent controls`) is declared by the program
    /// lowering, like every count of such a collection.
    fn requires_target_binding(&self) -> bool {
        match self {
            Self::Permanents(_) => false,
            Self::Cards(cards) => cards.requires_target_binding(),
            Self::Union(parts) => parts.iter().any(Self::requires_target_binding),
            Self::Object(
                super::PermanentEffectReference::Target(_)
                | super::PermanentEffectReference::TargetSet(_),
            ) => true,
            Self::Object(_) | Self::EventSpell | Self::ActionResult { .. } => false,
        }
    }

    /// Whether the set excludes the ability source (`other creatures you
    /// control`): only a permanent collection states it.
    fn excludes_source(&self) -> bool {
        match self {
            Self::Permanents(query) => query.exclude_source,
            Self::Union(parts) => parts.iter().any(Self::excludes_source),
            Self::Cards(_) | Self::Object(_) | Self::EventSpell | Self::ActionResult { .. } => {
                false
            }
        }
    }
}

impl ObjectAggregateNode {
    /// The aggregate as a numeric expression. A function other than
    /// `Distinct` over a non-numeric property is no number and is refused.
    pub(in crate::oracle::canonical) fn into_expression(self) -> Option<NumericExpression> {
        (self.function == AggregateFunction::Distinct || self.property.is_numeric())
            .then(|| NumericExpression::Quantity(Box::new(NumericQuantity::Aggregate(self))))
    }
}

/// The per-turn record of the resolving ability itself (CR 113.3, 602.1,
/// 603.1): its resolutions or its activations this turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AbilityHistoryMeasure {
    ResolutionsThisTurn,
    ActivationsThisTurn,
}

impl AbilityHistoryMeasure {
    /// The runtime numeric kind that counts this record.
    pub(in crate::oracle::canonical) const fn ir_kind(self) -> &'static str {
        match self {
            Self::ResolutionsThisTurn => "abilityResolutionsThisTurn",
            Self::ActivationsThisTurn => "abilityActivationsThisTurn",
        }
    }
}

/// `the total PROPERTY of COLLECTION`: one characteristic summed over a
/// collection the shared readers name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PropertyTotalNode {
    pub property: super::SelectorNumericProperty,
    pub collection: PropertyTotalCollection,
}

/// The collections a total reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PropertyTotalCollection {
    /// The objects an earlier action of the program affected (`permanents
    /// destroyed this way`, CR 608.2c), read as last known information once
    /// they left the battlefield (CR 608.2h). `reader` is the action whose
    /// argument reads the total; the program lowering binds the producer.
    ActionResult {
        reader: usize,
        reference: super::ActionResultReferenceNode,
    },
}

/// Where a counter query reads counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterHolder {
    /// The ability's own source.
    Source,
    /// Each permanent of a battlefield collection (`lands you control`).
    Permanents(super::ObjectQueryNode),
}

/// The objects an aggregate reads: cards in a zone of the shared scoped
/// query (`cards in your graveyard`), or the permanents the ability's
/// controller controls (`nonland permanents you control`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AggregateCollection {
    Cards(Box<super::ScopedObjectQueryNode>),
    Permanents(super::ObjectQueryNode),
}

impl AggregateCollection {
    fn uses_x(&self) -> bool {
        match self {
            Self::Cards(cards) => cards.uses_x(),
            Self::Permanents(query) => super::object_predicate_uses_x(&query.predicate),
        }
    }

    fn requires_target_binding(&self) -> bool {
        match self {
            Self::Cards(cards) => cards.requires_target_binding(),
            Self::Permanents(_) => false,
        }
    }
}

/// A characteristic whose distinct values a card collection aggregates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DistinctCardMeasure {
    /// CR 202.3.
    ManaValue,
}

impl NumericQuantity {
    pub(in crate::oracle::canonical) fn uses_x(&self) -> bool {
        match self {
            Self::Greatest(operands) => operands.iter().any(NumericExpression::uses_x),
            Self::Counters {
                holder: CounterHolder::Permanents(query),
                ..
            } => super::object_predicate_uses_x(&query.predicate),
            Self::DistinctCardValues { collection, .. }
            | Self::ManaSymbolsAmongCards { collection, .. } => collection.uses_x(),
            Self::Aggregate(node) => node.objects.uses_x(),
            Self::Difference(left, right) => left.uses_x() || right.uses_x(),
            Self::ManaSpentToCast { .. }
            | Self::CommanderCastsFromCommandZone
            | Self::ActingPlayerCards { .. }
            | Self::Players(_)
            | Self::AbilityHistory(_)
            | Self::PropertyTotal(_)
            | Self::PlayersComparedWithController { .. }
            | Self::PlayersAtExtremum { .. }
            | Self::ControllerMeasure(_)
            | Self::HighestCommanderTax
            | Self::CostChosenAmount
            | Self::Counters {
                holder: CounterHolder::Source,
                ..
            } => false,
        }
    }

    pub(in crate::oracle::canonical) fn requires_target_binding(&self) -> bool {
        match self {
            Self::Greatest(operands) => operands
                .iter()
                .any(NumericExpression::requires_target_binding),
            Self::DistinctCardValues { collection, .. }
            | Self::ManaSymbolsAmongCards { collection, .. } => {
                collection.requires_target_binding()
            }
            Self::Aggregate(node) => node.objects.requires_target_binding(),
            Self::Difference(left, right) => {
                left.requires_target_binding() || right.requires_target_binding()
            }
            Self::ManaSpentToCast { .. }
            | Self::CommanderCastsFromCommandZone
            | Self::ActingPlayerCards { .. }
            | Self::Players(_)
            | Self::AbilityHistory(_)
            | Self::PropertyTotal(_)
            | Self::PlayersComparedWithController { .. }
            | Self::PlayersAtExtremum { .. }
            | Self::ControllerMeasure(_)
            | Self::HighestCommanderTax
            | Self::CostChosenAmount
            | Self::Counters { .. } => false,
        }
    }

    /// Whether a count excludes the ability source (`other` in the holder).
    pub(in crate::oracle::canonical) fn excludes_source(&self) -> bool {
        match self {
            Self::Counters {
                holder: CounterHolder::Permanents(query),
                ..
            } => query.exclude_source,
            Self::Greatest(operands) => operands.iter().any(super::count_excludes_source),
            Self::Aggregate(node) => node.objects.excludes_source(),
            Self::Difference(left, right) => {
                super::count_excludes_source(left) || super::count_excludes_source(right)
            }
            Self::ManaSpentToCast { .. }
            | Self::CommanderCastsFromCommandZone
            | Self::Counters {
                holder: CounterHolder::Source,
                ..
            }
            | Self::DistinctCardValues { .. }
            | Self::ManaSymbolsAmongCards { .. }
            | Self::ActingPlayerCards { .. }
            | Self::Players(_)
            | Self::AbilityHistory(_)
            | Self::PropertyTotal(_)
            | Self::PlayersComparedWithController { .. }
            | Self::PlayersAtExtremum { .. }
            | Self::ControllerMeasure(_)
            | Self::HighestCommanderTax
            | Self::CostChosenAmount => false,
        }
    }
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(expected))
}

/// The mana a cast record distinguishes, named by its Oracle color word
/// (CR 105.1, 106.1b): the five colors and colorless.
fn spent_mana_symbol(raw: &str) -> Option<&'static str> {
    crate::oracle::characteristic_vocabulary::lookup_card_color(raw)
        .map(crate::oracle::characteristic_vocabulary::CardColor::ir_symbol)
        .or_else(|| raw.eq_ignore_ascii_case("colorless").then_some("C"))
}

/// Offset of `mana` after `the [total] amount of [COLOR]`, when the phrase
/// continues with `mana spent to cast`.
fn mana_spent_amount_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let mut offset = 1;
    if word_is(cursor, offset, "total") {
        offset += 1;
    }
    if !(word_is(cursor, 0, "the")
        && word_is(cursor, offset, "amount")
        && word_is(cursor, offset + 1, "of"))
    {
        return None;
    }
    offset += 2;
    if cursor
        .peek_ahead(offset)
        .and_then(spent_mana_symbol)
        .is_some()
    {
        offset += 1;
    }
    (word_is(cursor, offset, "mana")
        && word_is(cursor, offset + 1, "spent")
        && word_is(cursor, offset + 2, "to")
        && word_is(cursor, offset + 3, "cast"))
    .then_some(offset)
}

/// Lexical lookahead for `the [total] amount of [COLOR] mana spent to cast`.
pub(in crate::oracle::canonical) fn mana_spent_amount_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    mana_spent_amount_offset(cursor).is_some()
}

/// `the [total] amount of [COLOR] mana spent to cast SPELL` (CR 601.2h): the
/// total (or one color's) mana of a cast record. A mana type the record does
/// not distinguish (`{S}` from a snow source) is not read here.
pub(in crate::oracle::canonical) fn recognize_mana_spent_to_cast_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.mana-spent.amount");
    cursor.require(
        mana_spent_amount_starts(cursor),
        "`the amount of mana spent to cast`",
    )?;
    cursor.consume("the")?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("total"))
    {
        cursor.consume("total")?;
    }
    cursor.consume_phrase(&["amount", "of"])?;
    cursor.enter("numeric.mana-spent.symbol");
    let symbol = cursor.peek().and_then(spent_mana_symbol);
    if symbol.is_some() {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
    }
    cursor.consume_phrase(&["mana", "spent", "to", "cast"])?;
    let spell = recognize_cast_record_spell(cursor)?;
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::ManaSpentToCast { spell, symbol },
    )))
}

/// The spell whose cast record a quantity reads (CR 601.2h): `that spell`
/// or `it` after a cast event (the observed spell), else the ability's own
/// source by `this NOUN`, its name, or a pronoun whose antecedent is the
/// source. A pronoun never mints another object.
pub(in crate::oracle::canonical) fn recognize_cast_record_spell(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::CastSpellReference> {
    cursor.enter("numeric.cast-record.spell");
    let environment = super::installed_binding_environment(cursor);
    let event_spell = environment.frame.event_spell;
    if word_is(cursor, 0, "that") && word_is(cursor, 1, "spell") {
        cursor.require(
            event_spell,
            "a cast event that records the spell `that spell` names",
        )?;
        cursor.consume_phrase(&["that", "spell"])?;
        return Ok(super::CastSpellReference::EventSpell);
    }
    if word_is(cursor, 0, "this") && word_is(cursor, 1, "spell") {
        cursor.consume_phrase(&["this", "spell"])?;
        return Ok(super::CastSpellReference::Source(
            super::LocalObjectReference::SourceSelf,
        ));
    }
    if ["it", "her", "him"]
        .iter()
        .any(|pronoun| word_is(cursor, 0, pronoun))
    {
        let pronoun = cursor.peek().unwrap_or_default().to_owned();
        if event_spell && pronoun.eq_ignore_ascii_case("it") {
            cursor.consume("it")?;
            return Ok(super::CastSpellReference::EventSpell);
        }
        cursor.enter("numeric.cast-record.source-pronoun");
        let antecedent = super::resolve_possessive_antecedent(&environment)
            .map_err(|expected| cursor.fail(expected))?;
        cursor.require(
            antecedent
                == super::PermanentEffectReference::Source(super::LocalObjectReference::SourceSelf),
            "a pronoun that denotes the ability's own source",
        )?;
        cursor.consume(&pronoun)?;
        return Ok(super::CastSpellReference::Source(
            super::LocalObjectReference::SourceSelf,
        ));
    }
    let names = cursor.source_names().to_vec();
    super::recognize_source_reference(cursor, &names, None).map(super::CastSpellReference::Source)
}

/// `NUMBER or NUMBER, whichever is greater` (CR 107.1): lexical lookahead
/// after a left operand. The closing phrase must end the clause before a
/// sentence boundary.
pub(in crate::oracle::canonical) fn whichever_is_greater_follows(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "or")
        && (1..24)
            .take_while(|offset| {
                cursor
                    .peek_ahead(*offset)
                    .is_some_and(|raw| !matches!(raw, "." | ";" | ":"))
            })
            .any(|offset| {
                cursor.peek_ahead(offset) == Some(",")
                    && word_is(cursor, offset + 1, "whichever")
                    && word_is(cursor, offset + 2, "is")
                    && word_is(cursor, offset + 3, "greater")
            })
}

/// Closes `LEFT or RIGHT, whichever is greater` once the parent read RIGHT.
pub(in crate::oracle::canonical) fn recognize_whichever_is_greater_close(
    cursor: &mut OracleTokenCursor<'_>,
    left: NumericExpression,
    right: NumericExpression,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.greatest-of.close");
    cursor.consume_phrase(&[",", "whichever", "is", "greater"])?;
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::Greatest(vec![left, right]),
    )))
}

const HIGHEST_COMMANDER_TAX: &[&str] = &[
    "your",
    "highest",
    "commander",
    "tax",
    "among",
    "your",
    "commanders",
];

/// Lexical lookahead for `your highest commander tax among your commanders`.
pub(in crate::oracle::canonical) fn highest_commander_tax_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    HIGHEST_COMMANDER_TAX
        .iter()
        .enumerate()
        .all(|(offset, word)| word_is(cursor, offset, word))
}

/// `your highest commander tax among your commanders` (CR 903.8): the
/// commander record of the controller's commanders, read as their greatest
/// tax.
pub(in crate::oracle::canonical) fn recognize_highest_commander_tax(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.commander-tax.highest");
    cursor.consume_phrase(HIGHEST_COMMANDER_TAX)?;
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::HighestCommanderTax,
    )))
}

/// Lexical lookahead for the commander record countable: `time(s) you've
/// cast your commander|a commander from the command zone this game`.
pub(in crate::oracle::canonical) fn commander_cast_count_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    (word_is(cursor, 0, "time") || word_is(cursor, 0, "times"))
        && (word_is(cursor, 1, "you've")
            || (word_is(cursor, 1, "you") && word_is(cursor, 2, "have")))
        && (2..5).any(|offset| word_is(cursor, offset, "cast"))
        && (3..8).any(|offset| word_is(cursor, offset, "commander"))
}

/// `time(s) you've cast your commander|a commander from the command zone
/// this game` (CR 903.8): the number of times the controller cast one of
/// their commanders from the command zone. Only its owner casts a commander
/// from the command zone, so the record of the controller's commanders is
/// that count. Any other cast history is not this record and fails.
pub(in crate::oracle::canonical) fn recognize_commander_cast_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.commander-casts.noun");
    let noun = cursor.peek().unwrap_or_default().to_owned();
    cursor.require(
        noun.eq_ignore_ascii_case("time") || noun.eq_ignore_ascii_case("times"),
        "`time` or `times`",
    )?;
    cursor.consume(&noun)?;
    cursor.enter("numeric.commander-casts.actor");
    if word_is(cursor, 0, "you've") {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
    } else {
        cursor.consume_phrase(&["you", "have"])?;
    }
    cursor.enter("numeric.commander-casts.verb");
    cursor.consume("cast")?;
    cursor.enter("numeric.commander-casts.object");
    if word_is(cursor, 0, "your") {
        cursor.consume("your")?;
    } else {
        cursor.consume("a")?;
    }
    cursor.consume("commander")?;
    cursor.enter("numeric.commander-casts.zone");
    cursor.consume_phrase(&["from", "the", "command", "zone"])?;
    cursor.enter("numeric.commander-casts.scope");
    cursor.consume_phrase(&["this", "game"])?;
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::CommanderCastsFromCommandZone,
    )))
}

/// Lexical lookahead: `counter(s)` with no kind, followed by its holder.
pub(in crate::oracle::canonical) fn any_counter_operand_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    (word_is(cursor, 0, "counter") || word_is(cursor, 0, "counters"))
        && (word_is(cursor, 1, "on") || word_is(cursor, 1, "among"))
}

/// The holder of a counter query after `on`/`among`: the ability source by
/// `this NOUN`, its name or a pronoun naming it, else a battlefield
/// collection of the shared object query (`lands you control`). A collection
/// that names a target or a declared variable is not read here.
pub(in crate::oracle::canonical) fn recognize_counter_query_holder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterHolder> {
    cursor.enter("numeric.counter-holder.relation");
    let relation = cursor.peek().unwrap_or_default().to_owned();
    cursor.require(
        relation.eq_ignore_ascii_case("on") || relation.eq_ignore_ascii_case("among"),
        "`on` or `among` before the counters' holder",
    )?;
    cursor.consume(&relation)?;
    let on = relation.eq_ignore_ascii_case("on");
    let names = cursor.source_names().to_vec();
    let source_named = word_is(cursor, 0, "this")
        || (!names.is_empty()
            && super::source_reference_starts_at(cursor.position(), None, &names, None));
    if on && source_named {
        cursor.enter("numeric.counter-holder.source");
        let reference = super::recognize_source_reference(cursor, &names, None)?;
        cursor.require(
            reference == super::LocalObjectReference::SourceSelf,
            "the ability's own source",
        )?;
        return Ok(CounterHolder::Source);
    }
    if on
        && ["it", "him", "her"]
            .iter()
            .any(|pronoun| word_is(cursor, 0, pronoun))
    {
        cursor.enter("numeric.counter-holder.pronoun");
        let environment = super::installed_binding_environment(cursor);
        let antecedent = super::resolve_possessive_antecedent(&environment)
            .map_err(|expected| cursor.fail(expected))?;
        cursor.require(
            antecedent
                == super::PermanentEffectReference::Source(super::LocalObjectReference::SourceSelf),
            "a pronoun whose antecedent is the ability's source",
        )?;
        let pronoun = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&pronoun)?;
        return Ok(CounterHolder::Source);
    }
    recognize_counter_query_holder_collection(cursor)
}

/// A battlefield collection holding counters (`lands you control`): the
/// shared object query, without a target or declared variable.
pub(in crate::oracle::canonical) fn recognize_counter_query_holder_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterHolder> {
    cursor.enter("numeric.counter-holder.collection");
    let query = super::recognize_object_collection(cursor)?;
    cursor.require(
        query.denotes_battlefield_permanents(),
        "a counter holder collection anchored in the battlefield-permanent domain",
    )?;
    cursor.require(
        !super::object_predicate_uses_x(&query.predicate) && !query_names_target(&query),
        "a counter holder collection independent of targets and declared variables",
    )?;
    Ok(CounterHolder::Permanents(query))
}

/// A battlefield collection whose control relation names a declared target
/// (`creatures target player controls`) needs that declaration.
fn query_names_target(query: &super::ObjectQueryNode) -> bool {
    matches!(
        query.controller,
        Some(super::ObjectControllerScope::Player(_))
    )
}

/// `[KIND] counter(s) HOLDER` without a player holder: the counters of one
/// kind, or of every kind when no kind is named, on the holder.
pub(in crate::oracle::canonical) fn recognize_counter_quantity(
    cursor: &mut OracleTokenCursor<'_>,
    counter: Option<crate::oracle::counter_name::CounterName>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.counters.noun");
    let noun = cursor.peek().unwrap_or_default().to_owned();
    cursor.require(
        noun.eq_ignore_ascii_case("counter") || noun.eq_ignore_ascii_case("counters"),
        "`counter` or `counters`",
    )?;
    cursor.consume(&noun)?;
    let holder = recognize_counter_query_holder(cursor)?;
    Ok(match (holder, counter) {
        // The established source count keeps its existing typed form.
        (CounterHolder::Source, Some(counter)) => NumericExpression::CountCounters {
            object: super::LocalObjectReference::SourceSelf,
            counter,
        },
        (holder, counter) => {
            NumericExpression::Quantity(Box::new(NumericQuantity::Counters { counter, holder }))
        }
    })
}

/// Lexical lookahead for `different mana value(s) among`.
pub(in crate::oracle::canonical) fn distinct_card_value_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "different")
        && word_is(cursor, 1, "mana")
        && (word_is(cursor, 2, "value") || word_is(cursor, 2, "values"))
        && word_is(cursor, 3, "among")
}

/// `different mana value(s) among CARDS` (CR 202.3): the number of distinct
/// mana values among a card collection of the shared scoped query.
pub(in crate::oracle::canonical) fn recognize_distinct_card_values(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.distinct-values.measure");
    cursor.consume_phrase(&["different", "mana"])?;
    let noun = cursor.peek().unwrap_or_default().to_owned();
    cursor.require(
        noun.eq_ignore_ascii_case("value") || noun.eq_ignore_ascii_case("values"),
        "`value` or `values`",
    )?;
    cursor.consume(&noun)?;
    cursor.consume("among")?;
    cursor.enter("numeric.distinct-values.collection");
    let collection = recognize_aggregate_collection(cursor)?;
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::DistinctCardValues {
            measure: DistinctCardMeasure::ManaValue,
            collection,
        },
    )))
}

/// The collection an aggregate reads: cards in a zone of the shared scoped
/// query, else a battlefield collection the ability's controller controls.
/// Another controller, a source exclusion or a declared operand has no
/// aggregate evaluator and fails.
fn recognize_aggregate_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AggregateCollection> {
    let mut trial = cursor.clone();
    if let Ok(cards) = super::recognize_scoped_object_query(&mut trial)
        && cards.domain == super::ObjectKind::Card
        && cards.departures.is_none()
    {
        *cursor = trial;
        return Ok(AggregateCollection::Cards(Box::new(cards)));
    }
    cursor.enter("numeric.aggregate.permanents");
    let query = super::recognize_object_collection(cursor)?;
    cursor.require(
        query.denotes_battlefield_permanents()
            && query.controller == Some(super::ObjectControllerScope::AbilityController)
            && !query.exclude_source
            && !super::object_predicate_uses_x(&query.predicate),
        "cards in a zone, or permanents you control",
    )?;
    Ok(AggregateCollection::Permanents(query))
}

/// Lexical lookahead for `COLOR mana symbols in the mana costs of`.
pub(in crate::oracle::canonical) fn mana_symbols_among_cards_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .and_then(crate::oracle::characteristic_vocabulary::lookup_card_color)
        .is_some()
        && word_is(cursor, 1, "mana")
        && word_is(cursor, 2, "symbols")
        && word_is(cursor, 3, "in")
        && word_is(cursor, 4, "the")
        && word_is(cursor, 5, "mana")
        && word_is(cursor, 6, "costs")
        && word_is(cursor, 7, "of")
}

/// `COLOR mana symbols in the mana costs of CARDS` (CR 202.1): the symbols
/// of that color among the mana costs of a card collection; a hybrid or
/// Phyrexian symbol of the color counts once, like devotion (CR 700.5).
pub(in crate::oracle::canonical) fn recognize_mana_symbols_among_cards(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.mana-symbols.color");
    let color = cursor
        .peek()
        .and_then(crate::oracle::characteristic_vocabulary::lookup_card_color)
        .ok_or_else(|| cursor.fail("a color"))?;
    let raw = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&raw)?;
    cursor.enter("numeric.mana-symbols.costs");
    cursor.consume_phrase(&["mana", "symbols", "in", "the", "mana", "costs", "of"])?;
    cursor.enter("numeric.mana-symbols.collection");
    let collection = recognize_aggregate_collection(cursor)?;
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::ManaSymbolsAmongCards { color, collection },
    )))
}

/// `you've|you [have]|your opponents [have]` before `lost|gained this
/// turn` at `offset`: the offset of the verb and the player relation.
fn history_life_actor(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(usize, super::HistoryPlayer)> {
    let (verb, player) = if word_is(cursor, offset, "you've") {
        (offset + 1, super::HistoryPlayer::Controller)
    } else if word_is(cursor, offset, "you") {
        let have = usize::from(word_is(cursor, offset + 1, "have"));
        (offset + 1 + have, super::HistoryPlayer::Controller)
    } else if word_is(cursor, offset, "your") && word_is(cursor, offset + 1, "opponents") {
        let have = usize::from(word_is(cursor, offset + 2, "have"));
        (offset + 2 + have, super::HistoryPlayer::Opponent)
    } else {
        return None;
    };
    ((word_is(cursor, verb, "lost") || word_is(cursor, verb, "gained"))
        && word_is(cursor, verb + 1, "this")
        && word_is(cursor, verb + 2, "turn"))
    .then_some((verb, player))
}

/// Lexical lookahead for a player's life history amount: `the [total]
/// [amount of] life PLAYER [have] lost|gained this turn`.
fn history_life_offset(cursor: &OracleTokenCursor<'_>) -> Option<(usize, super::HistoryPlayer)> {
    let mut offset = 1;
    if word_is(cursor, offset, "total") {
        offset += 1;
    }
    if word_is(cursor, offset, "amount") && word_is(cursor, offset + 1, "of") {
        offset += 2;
    }
    if !(word_is(cursor, 0, "the") && word_is(cursor, offset, "life")) {
        return None;
    }
    history_life_actor(cursor, offset + 1)
}

pub(in crate::oracle::canonical) fn history_life_amount_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    history_life_offset(cursor).is_some()
}

/// Reads the life history verb and scope at the cursor (`lost this turn`)
/// as the life amount the recorded events of `player` total (CR 119.3).
fn recognize_life_history_tail(
    cursor: &mut OracleTokenCursor<'_>,
    player: super::HistoryPlayer,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.life-history.verb");
    let event = if word_is(cursor, 0, "lost") {
        cursor.consume("lost")?;
        super::HistoryEvent::LifeLost
    } else {
        cursor.consume("gained")?;
        super::HistoryEvent::LifeGained
    };
    cursor.enter("numeric.life-history.scope");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(NumericExpression::EventHistoryCount(Box::new(
        super::EventHistoryCountNode {
            pattern: super::EventHistoryPattern {
                event,
                player,
                object: None,
                measure: super::HistoryMeasure::Amount,
                qualifiers: super::HistoryQualifiers::default(),
            },
            scope: super::HistoryScope::ThisTurn,
            aggregate: super::HistoryAggregate::Total,
        },
    )))
}

/// `the [total] [amount of] life you've|your opponents have lost|gained
/// this turn` (CR 119.3): the life amount the turn's records total for that
/// player relation. A player the relation does not name (`they`, `that
/// player`) is not read here.
pub(in crate::oracle::canonical) fn recognize_history_life_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.life-history.amount");
    let (verb, player) = cursor.require_value(
        history_life_offset(cursor),
        "`the life you've lost this turn`",
    )?;
    let verb = cursor.position() + verb;
    cursor.advance_to(verb)?;
    recognize_life_history_tail(cursor, player)
}

/// `1 life PLAYER [have] lost|gained this turn` after `for each` (CR 119.3):
/// one per point of life, the same history amount.
pub(in crate::oracle::canonical) fn life_point_history_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "1") && word_is(cursor, 1, "life") && history_life_actor(cursor, 2).is_some()
}

pub(in crate::oracle::canonical) fn recognize_life_point_history(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.life-history.point");
    let (verb, player) = cursor.require_value(
        history_life_actor(cursor, 2),
        "`1 life you've lost this turn`",
    )?;
    let verb = cursor.position() + verb;
    cursor.advance_to(verb)?;
    recognize_life_history_tail(cursor, player)
}

/// Lexical lookahead for `the amount of damage SUBJECT dealt to RECIPIENT`.
pub(in crate::oracle::canonical) fn event_damage_amount_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "the")
        && word_is(cursor, 1, "amount")
        && word_is(cursor, 2, "of")
        && word_is(cursor, 3, "damage")
        // A subject precedes the verb (`the amount of damage it dealt`).
        && (5..9).any(|offset| word_is(cursor, offset, "dealt"))
}

/// `the amount of damage SUBJECT dealt to RECIPIENT` inside a trigger whose
/// event records one damage amount (CR 120.1, 603.2): that event's amount.
/// The subject is singular (the source by `this NOUN`, its name or a
/// pronoun) and the recipient the event's (`that player`, `it`); a trigger
/// without a recorded damage amount fails.
pub(in crate::oracle::canonical) fn recognize_event_damage_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.event-damage.amount");
    cursor.consume_phrase(&["the", "amount", "of", "damage"])?;
    cursor.enter("numeric.event-damage.subject");
    if ["it", "he", "she"]
        .iter()
        .any(|pronoun| word_is(cursor, 0, pronoun))
    {
        let pronoun = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&pronoun)?;
    } else {
        let names = cursor.source_names().to_vec();
        let reference = super::recognize_source_reference(cursor, &names, None)?;
        cursor.require(
            reference == super::LocalObjectReference::SourceSelf,
            "the ability's own source as the damage source",
        )?;
    }
    cursor.enter("numeric.event-damage.verb");
    cursor.consume("dealt")?;
    cursor.enter("numeric.event-damage.recipient");
    cursor.consume("to")?;
    if word_is(cursor, 0, "that") {
        cursor.consume("that")?;
        let noun = cursor.peek().unwrap_or_default().to_owned();
        cursor.require(
            ["player", "creature", "permanent", "planeswalker"]
                .iter()
                .any(|expected| noun.eq_ignore_ascii_case(expected)),
            "the recipient the damage event records",
        )?;
        cursor.consume(&noun)?;
    } else {
        let pronoun = cursor.peek().unwrap_or_default().to_owned();
        cursor.require(
            ["it", "them", "him", "her"]
                .iter()
                .any(|expected| pronoun.eq_ignore_ascii_case(expected)),
            "the recipient the damage event records",
        )?;
        cursor.consume(&pronoun)?;
    }
    cursor.enter("numeric.event-damage.binding");
    cursor.require(
        cursor.event_numeric_binding() == Some(EventNumericBinding::DamageAmount),
        "a damage event that records the amount dealt",
    )?;
    Ok(NumericExpression::EventValue(
        EventNumericBinding::DamageAmount,
    ))
}

/// Offset of `excess` in `the [amount of] excess damage dealt`.
fn excess_damage_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let offset = if word_is(cursor, 1, "amount") && word_is(cursor, 2, "of") {
        3
    } else {
        1
    };
    (word_is(cursor, 0, "the")
        && word_is(cursor, offset, "excess")
        && word_is(cursor, offset + 1, "damage")
        && word_is(cursor, offset + 2, "dealt"))
    .then_some(offset)
}

/// Lexical lookahead for `the [amount of] excess damage dealt`.
pub(in crate::oracle::canonical) fn excess_damage_result_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    excess_damage_offset(cursor).is_some()
}

/// `the [amount of] excess damage dealt [to that creature|to it] this way`
/// (CR 120.4a, 608.2c): the damage beyond lethal the nearest earlier damage
/// action of the program dealt to a permanent. The program lowering binds
/// that action or fails; the first action of a program has none.
pub(in crate::oracle::canonical) fn recognize_excess_damage_result(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.excess-damage.quantity");
    let offset = cursor.require_value(
        excess_damage_offset(cursor),
        "`the excess damage dealt this way`",
    )?;
    let at = cursor.position() + offset;
    cursor.advance_to(at)?;
    cursor.consume_phrase(&["excess", "damage", "dealt"])?;
    if word_is(cursor, 0, "to") {
        cursor.enter("numeric.excess-damage.recipient");
        cursor.consume("to")?;
        if word_is(cursor, 0, "that") {
            cursor.consume("that")?;
            let noun = cursor.peek().unwrap_or_default().to_owned();
            cursor.require(
                ["creature", "permanent", "planeswalker"]
                    .iter()
                    .any(|expected| noun.eq_ignore_ascii_case(expected)),
                "the permanent the damage action dealt damage to",
            )?;
            cursor.consume(&noun)?;
        } else {
            cursor.consume("it")?;
        }
    }
    cursor.enter("numeric.excess-damage.scope");
    cursor.consume_phrase(&["this", "way"])?;
    cursor.enter("numeric.excess-damage.reader");
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .filter(|index| *index > 0);
    let reader = cursor.require_value(
        reader,
        "an earlier damage action of this program whose excess damage the phrase restates",
    )?;
    Ok(NumericExpression::ActionResultQuantity {
        reader_effect: reader,
        quantity: super::ActionResultQuantity::ExcessDamage,
    })
}

/// The width of the partitive `the cards in` before `their ZONE`.
fn acting_player_zone_offset(cursor: &OracleTokenCursor<'_>) -> usize {
    if word_is(cursor, 0, "the") && word_is(cursor, 1, "cards") && word_is(cursor, 2, "in") {
        3
    } else {
        0
    }
}

/// Lexical lookahead for `their library`, `the cards in their hand|library`:
/// the acting player's cards in one zone.
pub(in crate::oracle::canonical) fn acting_player_zone_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let offset = acting_player_zone_offset(cursor);
    word_is(cursor, offset, "their")
        && (word_is(cursor, offset + 1, "library")
            || (offset > 0 && word_is(cursor, offset + 1, "hand")))
}

/// `their library` / `the cards in their hand|library` as a number of cards
/// (`mills half their library`, `discards half the cards in their hand`):
/// the cards in that zone of the player who performs the action (CR 401.1,
/// 402.1). The action's lowering names that player; nothing else binds
/// `their`.
pub(in crate::oracle::canonical) fn recognize_acting_player_zone(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.acting-player-zone");
    if acting_player_zone_offset(cursor) > 0 {
        cursor.consume_phrase(&["the", "cards", "in"])?;
    }
    cursor.consume("their")?;
    let zone = if word_is(cursor, 0, "hand") {
        cursor.consume("hand")?;
        super::LocalZone::Hand
    } else {
        cursor.consume("library")?;
        super::LocalZone::Library
    };
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::ActingPlayerCards { zone },
    )))
}

/// Whether a count already names the cards it counts (`half their library`),
/// so the action reads no `cards` noun after it.
pub(in crate::oracle::canonical) fn count_names_its_cards(count: &NumericExpression) -> bool {
    match count {
        NumericExpression::Quantity(quantity) => {
            matches!(quantity.as_ref(), NumericQuantity::ActingPlayerCards { .. })
        }
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            count_names_its_cards(left) || count_names_its_cards(right)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "numeric_expression_tests.rs"]
mod expression_tests;

#[cfg(test)]
#[path = "numeric_qualified_target_tests.rs"]
mod qualified_target_tests;
