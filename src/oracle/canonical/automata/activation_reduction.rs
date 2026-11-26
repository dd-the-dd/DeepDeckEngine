//! `This ability costs DELTA less to activate [AMOUNT TAIL] [CONDITION].`
//!
//! The activated-ability subject of the one cost-adjustment frame (CR 601.2f
//! via 602.2b). The delta, the `for each` and `where X is` amounts and the
//! condition are the same children the casting reduction reads; this parent
//! owns only the `this ability ... to activate` envelope. It returns
//! metadata, never an effect node.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CostAdjustmentCondition, ManaCostNode, NumericExpression, ObjectControllerScope,
    ObjectPredicate, ObjectQueryNode, ReductionDelta, recognize_cost_adjustment_condition,
    recognize_mana_cost, recognize_object_collection, recognize_object_query,
    recognize_optional_amount_reminder, recognize_source_reference, recognize_where_x_definition,
    where_x_definition_starts,
};

/// The amount an activation reduction removes from the generic component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationReductionAmount {
    /// `{N} less to activate for each PERMANENT you control`.
    PerControlledPermanent {
        generic: i64,
        selector: ObjectPredicate,
    },
    /// `{N} less`, or `{X} less ..., where X is EXPR` (CR 107.3).
    Expression(NumericExpression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivationCostReductionNode {
    pub amount: ActivationReductionAmount,
    /// `if COND`, `during your turn`, `if it targets a QUERY`.
    pub condition: Option<CostAdjustmentCondition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivationReductionState {
    Subject,
    Action,
    Amount,
    Reduction,
    Quantifier,
    Selector,
    Condition,
    Terminator,
    Complete,
}

/// Recognizes the activation cost reduction metadata of one activated
/// ability.
pub(in crate::oracle::canonical) fn recognize_activation_cost_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationCostReductionNode> {
    let mut state = ActivationReductionState::Subject;
    let mut delta = None;
    let mut amount = None;
    let mut condition = None;
    loop {
        state = match state {
            ActivationReductionState::Subject => {
                cursor.enter("activation-reduction.subject");
                cursor.consume_phrase(&["this", "ability"])?;
                ActivationReductionState::Action
            }
            ActivationReductionState::Action => {
                cursor.enter("activation-reduction.action");
                cursor.consume("costs")?;
                ActivationReductionState::Amount
            }
            ActivationReductionState::Amount => {
                cursor.enter("activation-reduction.amount");
                if cursor
                    .peek_token()
                    .is_none_or(|token| token.token_type != "manaSymbol")
                {
                    return Err(cursor.fail("a positive generic mana symbol or {X}"));
                }
                let cost = recognize_mana_cost(cursor)?;
                cursor.enter("activation-reduction.amount");
                delta = Some(cursor.require_value(
                    ReductionDelta::of(&cost),
                    "a positive generic mana symbol or {X}",
                )?);
                ActivationReductionState::Reduction
            }
            ActivationReductionState::Reduction => {
                cursor.enter("activation-reduction.reduction");
                cursor.consume_phrase(&["less", "to", "activate"])?;
                ActivationReductionState::Quantifier
            }
            ActivationReductionState::Quantifier => {
                cursor.enter("activation-reduction.quantifier");
                match delta {
                    Some(ReductionDelta::Variable) => {
                        cursor.require(
                            where_x_definition_starts(cursor),
                            "`, where X is` defining the {X} reduction",
                        )?;
                        amount = Some(ActivationReductionAmount::Expression(
                            recognize_where_x_definition(cursor)?,
                        ));
                        ActivationReductionState::Condition
                    }
                    Some(ReductionDelta::Generic(_))
                        if cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("for")) =>
                    {
                        cursor.consume_phrase(&["for", "each"])?;
                        ActivationReductionState::Selector
                    }
                    Some(ReductionDelta::Generic(generic)) => {
                        amount = Some(ActivationReductionAmount::Expression(
                            NumericExpression::from(i64::from(generic)),
                        ));
                        ActivationReductionState::Condition
                    }
                    None => return Err(cursor.fail("a populated reduction delta")),
                }
            }
            ActivationReductionState::Selector => {
                cursor.enter("activation-reduction.selector");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller == Some(ObjectControllerScope::AbilityController),
                    "a permanent selector controlled by the ability controller",
                )?;
                cursor.require(
                    query.denotes_battlefield_permanents(),
                    "a battlefield-permanent selector",
                )?;
                let Some(ReductionDelta::Generic(generic)) = delta else {
                    return Err(cursor.fail("a generic reduction per permanent"));
                };
                amount = Some(ActivationReductionAmount::PerControlledPermanent {
                    generic: i64::from(generic),
                    selector: query.predicate,
                });
                ActivationReductionState::Condition
            }
            ActivationReductionState::Condition => {
                cursor.enter("activation-reduction.condition");
                condition = recognize_cost_adjustment_condition(cursor)?;
                ActivationReductionState::Terminator
            }
            ActivationReductionState::Terminator => {
                cursor.enter("activation-reduction.terminator");
                cursor.consume(".")?;
                if let Some(ActivationReductionAmount::Expression(expression)) = &amount {
                    recognize_optional_amount_reminder(cursor, expression)?;
                }
                ActivationReductionState::Complete
            }
            ActivationReductionState::Complete => {
                cursor.enter("activation-reduction.complete");
                return Ok(ActivationCostReductionNode {
                    amount: cursor
                        .require_value(amount, "a populated activation reduction amount")?,
                    condition,
                });
            }
        };
    }
}

/// The activated abilities a static cost modification applies to (CR 602.2b
/// applies CR 601.2f to activation): the ability class, the activating
/// players, the sources whose abilities they are, and the objects the
/// ability targets. Every part is optional and independent.
///
/// ```text
/// Population := [CLASS] "abilities" Tail*
///             | KEYWORD "costs" ["you" "pay"]                (keyword costs)
/// CLASS      := "activated" | "equip" | "loyalty" | "boast" | "ninjutsu"
///             | "cycling" | "power-up" | "exhaust" | "forecast"
/// Tail       := "of" PERMANENTS | ACTIVATORS "activate" | "that" "target" OBJECT
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivatedAbilityPopulationNode {
    /// The engine's ability class (`equip`, `loyalty`, `boast`, ...);
    /// `None` is every activated ability.
    pub class: Option<&'static str>,
    /// `you activate`, `your opponents activate`.
    pub activators: Option<AbilityActivatorScope>,
    /// `of PERMANENTS`: the permanents whose abilities they are.
    pub sources: Option<ObjectQueryNode>,
    /// `that target OBJECT` (CR 115.1): matched against the chosen targets,
    /// which are chosen before the total cost (CR 601.2c, 602.2b).
    pub target: Option<AbilityTargetCondition>,
    /// `unless they're mana abilities`, a keyword class or a target: the
    /// population holds no mana ability (CR 605.1a).
    pub excludes_mana_abilities: bool,
    /// `Buyback costs`, `Equip costs you pay`: a keyword's cost named
    /// directly rather than its ability.
    pub keyword_cost: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AbilityActivatorScope {
    Controller,
    Opponents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AbilityTargetCondition {
    /// `that target this creature`: the static ability's own source.
    Source,
    /// `that target a Merfolk you control`.
    Query(ObjectQueryNode),
}

/// The direction and amount of an activation cost modification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationCostDeltaNode {
    /// `{N} less`: generic mana only (CR 601.2f).
    Reduce(i32),
    /// `{N} more`, `{W} more`: an added mana cost.
    Increase(ManaCostNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivationCostModificationNode {
    pub abilities: ActivatedAbilityPopulationNode,
    pub delta: ActivationCostDeltaNode,
    pub condition: Option<CostAdjustmentCondition>,
    /// `This effect can't reduce the mana in that cost to less than one
    /// mana.`: a floor applied after the reduction.
    pub minimum_one_mana: bool,
}

pub(in crate::oracle::canonical) enum ActivationCostModificationAttempt {
    NotClaimed,
    Parsed(ActivationCostModificationNode),
    FailedCommitted(super::super::MachineFailure),
}

fn word(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(expected))
}

/// The ability class word at the cursor and its token width.
fn ability_class_at(cursor: &OracleTokenCursor<'_>) -> Option<(Option<&'static str>, usize)> {
    const CLASSES: [(&str, Option<&str>); 9] = [
        ("activated", None),
        ("equip", Some("equip")),
        ("loyalty", Some("loyalty")),
        ("boast", Some("boast")),
        ("ninjutsu", Some("ninjutsu")),
        ("cycling", Some("cycling")),
        ("power-up", Some("powerUp")),
        ("exhaust", Some("exhaust")),
        ("forecast", Some("forecast")),
    ];
    if word(cursor, 0, "power") && word(cursor, 1, "-") && word(cursor, 2, "up") {
        return Some((Some("powerUp"), 3));
    }
    CLASSES
        .iter()
        .find(|(raw, _)| word(cursor, 0, raw))
        .map(|(_, class)| (*class, 1))
}

/// `you activate` / `your opponents activate`, and its width, at `offset`.
fn activator_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(AbilityActivatorScope, usize)> {
    if word(cursor, offset, "you") && word(cursor, offset + 1, "activate") {
        Some((AbilityActivatorScope::Controller, 2))
    } else if word(cursor, offset, "your")
        && word(cursor, offset + 1, "opponents")
        && word(cursor, offset + 2, "activate")
    {
        Some((AbilityActivatorScope::Opponents, 3))
    } else {
        None
    }
}

/// `KEYWORD costs` naming a keyword's cost (CR 702): `Equip costs`,
/// `Buyback costs`, `Dash costs`, `Unlock costs`.
fn keyword_cost_class_at(cursor: &OracleTokenCursor<'_>) -> Option<(&'static str, usize)> {
    const KEYWORD_COSTS: [(&str, &str); 4] = [
        ("equip", "equip"),
        ("buyback", "buyback"),
        ("dash", "dash"),
        ("unlock", "unlock"),
    ];
    KEYWORD_COSTS
        .iter()
        .find(|(raw, _)| word(cursor, 0, raw) && word(cursor, 1, "costs"))
        .map(|(_, class)| (*class, 2))
}

/// Whether the cursor starts an ability population (`Equip abilities`,
/// `Activated abilities`, `Abilities your opponents activate`) or a keyword
/// cost population (`Equip costs`, `Buyback costs`).
pub(in crate::oracle::canonical) fn activation_population_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    if keyword_cost_class_at(cursor).is_some() {
        return true;
    }
    if word(cursor, 0, "abilities") {
        return activator_at(cursor, 1).is_some();
    }
    ability_class_at(cursor).is_some_and(|(_, width)| word(cursor, width, "abilities"))
}

fn recognize_ability_population(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
) -> MachineResult<ActivatedAbilityPopulationNode> {
    cursor.enter("activation-population.class");
    if let Some((class, width)) = keyword_cost_class_at(cursor) {
        cursor.advance_to(cursor.position() + width)?;
        let activators = if word(cursor, 0, "you") && word(cursor, 1, "pay") {
            cursor.consume_phrase(&["you", "pay"])?;
            Some(AbilityActivatorScope::Controller)
        } else {
            None
        };
        return Ok(ActivatedAbilityPopulationNode {
            class: Some(class),
            activators,
            sources: None,
            target: None,
            excludes_mana_abilities: true,
            keyword_cost: true,
        });
    }
    let class = if word(cursor, 0, "abilities") {
        None
    } else {
        let (class, width) =
            ability_class_at(cursor).ok_or_else(|| cursor.fail("an ability class word"))?;
        cursor.advance_to(cursor.position() + width)?;
        class
    };
    cursor.enter("activation-population.noun");
    cursor.consume("abilities")?;
    let mut population = ActivatedAbilityPopulationNode {
        class,
        activators: None,
        sources: None,
        target: None,
        // A keyword class never names a mana ability.
        excludes_mana_abilities: class.is_some(),
        keyword_cost: false,
    };
    loop {
        if word(cursor, 0, "of") && population.sources.is_none() {
            cursor.enter("activation-population.sources");
            cursor.consume("of")?;
            let sources = recognize_object_collection(cursor)?;
            cursor.require(
                sources.denotes_battlefield_permanents(),
                "the battlefield permanents whose abilities are activated",
            )?;
            population.sources = Some(sources);
            continue;
        }
        if population.activators.is_none()
            && let Some((scope, width)) = activator_at(cursor, 0)
        {
            cursor.enter("activation-population.activators");
            cursor.advance_to(cursor.position() + width)?;
            population.activators = Some(scope);
            continue;
        }
        if population.target.is_none() && word(cursor, 0, "that") && word(cursor, 1, "target") {
            cursor.enter("activation-population.target");
            cursor.consume_phrase(&["that", "target"])?;
            population.target = Some(recognize_ability_target(cursor, name_links, name_failure)?);
            // An ability with a target is not a mana ability (CR 605.1a).
            population.excludes_mana_abilities = true;
            continue;
        }
        break;
    }
    Ok(population)
}

/// `that target this creature|NAME|a|an OBJECT`: the targeted object of a
/// spell or ability population, after `that target` (CR 115.1).
pub(in crate::oracle::canonical) fn recognize_target_relative(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
) -> MachineResult<AbilityTargetCondition> {
    cursor.enter("target-relative.verb");
    cursor.consume_phrase(&["that", "target"])?;
    cursor.enter("target-relative.object");
    recognize_ability_target(cursor, name_links, name_failure)
}

/// Whether `that target` follows.
pub(in crate::oracle::canonical) fn target_relative_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    word(cursor, 0, "that") && word(cursor, 1, "target")
}

/// `this creature` / the source's name, or `a|an OBJECT`.
fn recognize_ability_target(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
) -> MachineResult<AbilityTargetCondition> {
    if word(cursor, 0, "a") || word(cursor, 0, "an") {
        cursor.advance_to(cursor.position() + 1)?;
        let query = recognize_object_query(cursor)?;
        cursor.require(
            query.denotes_battlefield_permanents(),
            "a targeted permanent",
        )?;
        return Ok(AbilityTargetCondition::Query(query));
    }
    recognize_source_reference(cursor, name_links, name_failure)?;
    Ok(AbilityTargetCondition::Source)
}

/// `ABILITIES cost DELTA less|more [to activate] [unless they're mana
/// abilities] [CONDITION]. [FLOOR]` (CR 602.2b, 601.2f). Once the
/// population and its `cost` verb are read, the frame owns the rest.
pub(in crate::oracle::canonical) fn try_recognize_activation_cost_modification(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
) -> ActivationCostModificationAttempt {
    if !activation_population_starts(cursor) {
        return ActivationCostModificationAttempt::NotClaimed;
    }
    let mut trial = cursor.clone();
    let abilities = match recognize_ability_population(&mut trial, name_links, name_failure) {
        Ok(abilities) if word(&trial, 0, "cost") => abilities,
        _ => return ActivationCostModificationAttempt::NotClaimed,
    };
    match recognize_activation_cost_tail(&mut trial, abilities) {
        Ok(node) => {
            *cursor = trial;
            ActivationCostModificationAttempt::Parsed(node)
        }
        Err(failure) => ActivationCostModificationAttempt::FailedCommitted(failure),
    }
}

fn recognize_activation_cost_tail(
    cursor: &mut OracleTokenCursor<'_>,
    mut abilities: ActivatedAbilityPopulationNode,
) -> MachineResult<ActivationCostModificationNode> {
    cursor.enter("activation-cost.verb");
    cursor.consume("cost")?;
    cursor.enter("activation-cost.delta");
    let cost = recognize_mana_cost(cursor)?;
    let delta = if word(cursor, 0, "less") {
        cursor.consume("less")?;
        match ReductionDelta::of(&cost) {
            Some(ReductionDelta::Generic(amount)) => ActivationCostDeltaNode::Reduce(amount),
            _ => return Err(cursor.fail("one positive generic mana symbol")),
        }
    } else {
        cursor.consume("more")?;
        cursor.require(
            !cost.contains_variable_x(),
            "a fixed added mana cost without {X}",
        )?;
        ActivationCostDeltaNode::Increase(cost)
    };
    cursor.enter("activation-cost.action");
    if abilities.keyword_cost {
        cursor.require(
            !word(cursor, 0, "to"),
            "a keyword cost without `to activate`",
        )?;
    } else {
        cursor.consume_phrase(&["to", "activate"])?;
    }
    if word(cursor, 0, "unless") {
        cursor.enter("activation-cost.mana-ability-exception");
        cursor.consume("unless")?;
        let width = if word(cursor, 0, "they're") {
            1
        } else {
            cursor.require(
                word(cursor, 0, "they") && word(cursor, 1, "are"),
                "`they're mana abilities`",
            )?;
            2
        };
        cursor.advance_to(cursor.position() + width)?;
        cursor.consume_phrase(&["mana", "abilities"])?;
        abilities.excludes_mana_abilities = true;
    }
    let condition = recognize_cost_adjustment_condition(cursor)?;
    cursor.enter("activation-cost.condition");
    cursor.require(
        !matches!(condition, Some(CostAdjustmentCondition::Targets(_))),
        "a sentence condition (a targeted object belongs to the population)",
    )?;
    cursor.enter("activation-cost.terminator");
    cursor.consume(".")?;
    let minimum_one_mana = word(cursor, 0, "this") && word(cursor, 1, "effect");
    if minimum_one_mana {
        cursor.enter("activation-cost.floor");
        cursor.require(
            matches!(delta, ActivationCostDeltaNode::Reduce(_)),
            "a floor after a reduction",
        )?;
        cursor.consume_phrase(&["this", "effect"])?;
        cursor.require(
            word(cursor, 0, "can't") || word(cursor, 0, "cannot"),
            "`can't reduce`",
        )?;
        cursor.advance_to(cursor.position() + 1)?;
        cursor.consume_phrase(&["reduce", "the", "mana", "in", "that"])?;
        if word(cursor, 0, "cost") {
            cursor.consume("cost")?;
        } else {
            cursor.require(
                word(cursor, 0, "ability's"),
                "`that cost` or `that ability's activation cost`",
            )?;
            cursor.advance_to(cursor.position() + 1)?;
            cursor.consume_phrase(&["activation", "cost"])?;
        }
        cursor.consume_phrase(&["to", "less", "than", "one", "mana", "."])?;
    }
    cursor.enter("activation-cost.complete");
    cursor.finish()?;
    Ok(ActivationCostModificationNode {
        abilities,
        delta,
        condition,
        minimum_one_mana,
    })
}
