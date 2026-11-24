//! Characteristics belong to one shared token descriptor, not a named catalog lookup.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_card_type, lookup_creature_subtype_tokens,
    lookup_noncreature_subtype_forms,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ExplicitTokenTemplateNode {
    /// The literal creature specification shared with the older token
    /// parents. When `statistics` is not a literal pair, its power and
    /// toughness are not read: every consumer of a variable pair reads
    /// `statistics` (see [`Self::literal_statistics`]).
    pub creature: CreatureTokenSpecNode,
    pub types: Vec<ObjectCardType>,
    /// The token's power and toughness as shared numeric expressions
    /// (`X/X`, with a sequence-owned `where X is EXPR`, CR 107.3). `None`
    /// when the description states none: a noncreature token, or a
    /// creature whose characteristic-defining ability defines them
    /// (CR 604.3); the creation body proves that ability.
    pub statistics: Option<BasePowerToughnessNode>,
    /// Every subtype in printed order (CR 205.3): creature subtypes and
    /// noncreature subtypes (`Planet`, `Forest`, `Vehicle`) alike.
    pub subtypes: Vec<&'static str>,
}

impl ExplicitTokenTemplateNode {
    /// Whether the stated power and toughness are literal integers, the only
    /// form `creature` represents.
    pub(in crate::oracle::canonical) fn literal_statistics(&self) -> bool {
        self.statistics
            .as_ref()
            .is_some_and(|statistics| statistics.literal().is_some())
    }

    /// Whether the description makes a creature (CR 111.4).
    pub(in crate::oracle::canonical) fn is_creature(&self) -> bool {
        self.types.contains(&ObjectCardType::Creature)
    }

    /// The type line the token's quoted abilities are compiled with: its
    /// card types and subtypes (CR 205.1a), so a quoted `this artifact` or
    /// `this land` agrees with the object that has the ability.
    pub(in crate::oracle::canonical) fn granted_type_line(&self) -> String {
        let types = self
            .types
            .iter()
            .map(|kind| kind.ir_name())
            .collect::<Vec<_>>()
            .join(" ");
        if self.subtypes.is_empty() {
            types
        } else {
            format!("{types} \u{2014} {}", self.subtypes.join(" "))
        }
    }
}

/// The battlefield entry state of a created token (CR 111.1, 508.4): it
/// enters tapped, attacking, or both. `tapped and attacking` is one state
/// phrase, not a coordination of two token descriptions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TokenEntryStateNode {
    pub tapped: bool,
    pub attacking: bool,
}

impl TokenEntryStateNode {
    fn is_stated(self) -> bool {
        self.tapped || self.attacking
    }
}

/// Battlefield entry state attached to a catalog token phrase. Only the
/// tapped state is read here: a catalog identity never enters attacking in
/// Oracle text, so `tapped and attacking` fails explicitly at its connector.
pub(in crate::oracle::canonical) fn recognize_token_entry_state(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    cursor.enter("token.entry-state");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"))
    {
        cursor.consume("tapped")?;
        return Ok(true);
    }
    Ok(false)
}

/// The entry state written before a token description: `tapped`,
/// `attacking`, or `tapped and attacking` (CR 508.4). The connector belongs
/// to the state phrase only when `attacking` follows it.
pub(in crate::oracle::canonical) fn recognize_token_entry_prefix(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TokenEntryStateNode> {
    cursor.enter("token.entry-state");
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let mut state = TokenEntryStateNode::default();
    if word(cursor, 0, "tapped") {
        cursor.consume("tapped")?;
        state.tapped = true;
        if word(cursor, 0, "and") && word(cursor, 1, "attacking") {
            cursor.consume_phrase(&["and", "attacking"])?;
            state.attacking = true;
        }
    } else if word(cursor, 0, "attacking") {
        cursor.consume("attacking")?;
        state.attacking = true;
    }
    Ok(state)
}

/// The entry state written after the token noun: `that's|that are [tapped
/// and] attacking`, `that's|that are tapped` (CR 508.4). `None` when no such
/// relative clause starts; the clause is owned only when its whole state
/// phrase is read.
pub(in crate::oracle::canonical) fn recognize_token_entry_clause(
    cursor: &mut OracleTokenCursor<'_>,
    singular: bool,
) -> MachineResult<Option<TokenEntryStateNode>> {
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let width = if word(cursor, 0, "that's") || word(cursor, 0, "that\u{2019}s") {
        1
    } else if word(cursor, 0, "that") && (word(cursor, 1, "are") || word(cursor, 1, "is")) {
        2
    } else {
        return Ok(None);
    };
    let starts_state = word(cursor, width, "tapped") || word(cursor, width, "attacking");
    if !starts_state {
        return Ok(None);
    }
    cursor.enter("token.entry-clause.copula");
    let plural_copula = width == 2 && word(cursor, 1, "are");
    cursor.require(
        plural_copula != singular,
        "a relative copula agreeing with the token quantity",
    )?;
    cursor.advance_to(cursor.position() + width)?;
    let state = recognize_token_entry_prefix(cursor)?;
    cursor.enter("token.entry-clause.complete");
    cursor.require(state.is_stated(), "a tapped or attacking entry state")?;
    Ok(Some(state))
}

/// The colors written after the token noun (CR 105.1, 111.4): `that's all
/// colors`, `that are red, green, and white`. `None` when no such relative
/// clause starts; an entry-state clause is read by its own reader.
pub(in crate::oracle::canonical) fn recognize_token_color_clause(
    cursor: &mut OracleTokenCursor<'_>,
    singular: bool,
) -> MachineResult<Option<Vec<CardColor>>> {
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let width = if word(cursor, 0, "that's") || word(cursor, 0, "that\u{2019}s") {
        1
    } else if word(cursor, 0, "that") && (word(cursor, 1, "are") || word(cursor, 1, "is")) {
        2
    } else {
        return Ok(None);
    };
    let all_colors = word(cursor, width, "all") && word(cursor, width + 1, "colors");
    let color_list = cursor
        .peek_ahead(width)
        .and_then(card_color_from_oracle_word)
        .is_some();
    if !(all_colors || color_list) {
        return Ok(None);
    }
    cursor.enter("token.color-clause.copula");
    let plural_copula = width == 2 && word(cursor, 1, "are");
    cursor.require(
        plural_copula != singular,
        "a relative copula agreeing with the token quantity",
    )?;
    cursor.advance_to(cursor.position() + width)?;
    cursor.enter("token.color-clause.colors");
    if all_colors {
        cursor.consume_phrase(&["all", "colors"])?;
        return Ok(Some(vec![
            CardColor::White,
            CardColor::Blue,
            CardColor::Black,
            CardColor::Red,
            CardColor::Green,
        ]));
    }
    let mut colors = Vec::new();
    loop {
        let color = cursor
            .peek()
            .and_then(card_color_from_oracle_word)
            .ok_or_else(|| cursor.fail("a registered Magic color"))?;
        cursor.require(!colors.contains(&color), "distinct token colors")?;
        colors.push(color);
        cursor.advance_to(cursor.position() + 1)?;
        let mut next = cursor.clone();
        if next.peek() == Some(",") {
            next.consume(",")?;
        }
        if next
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
        {
            next.consume("and")?;
        }
        if next.position() == cursor.position()
            || next.peek().and_then(card_color_from_oracle_word).is_none()
        {
            break;
        }
        *cursor = next;
    }
    Ok(Some(colors))
}

pub(in crate::oracle::canonical) enum TokenDescriptorAttempt {
    NotClaimed,
    Recognized(ExplicitTokenTemplateNode),
    FailedCommitted(super::super::MachineFailure),
}

#[derive(Clone, Copy)]
enum DescriptorState {
    Statistics,
    Colors,
    Subtypes,
    Types,
    Complete,
}

/// The P/T lexical atom or a color word introduces this descriptor. A
/// description without either (`a Forest land token`) is claimed only when
/// it reads completely up to its `token(s)` noun with a card type, so a
/// catalog identity (`a Treasure token`) stays the named child's input. No
/// cursor-distance or slash scan decides ownership.
pub(in crate::oracle::canonical) fn try_recognize_token_descriptor(
    cursor: &mut OracleTokenCursor<'_>,
) -> TokenDescriptorAttempt {
    let introduced = cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "counterValue")
        || numeric_power_toughness_starts(cursor)
        || descriptor_color_starts(cursor);
    if introduced {
        return match recognize_token_descriptor(cursor) {
            Ok(node) => TokenDescriptorAttempt::Recognized(node),
            Err(failure) => TokenDescriptorAttempt::FailedCommitted(failure),
        };
    }
    let mut trial = cursor.clone();
    match recognize_token_descriptor(&mut trial) {
        Ok(node)
            if trial.peek().is_some_and(|raw| {
                raw.eq_ignore_ascii_case("token") || raw.eq_ignore_ascii_case("tokens")
            }) =>
        {
            *cursor = trial;
            TokenDescriptorAttempt::Recognized(node)
        }
        _ => TokenDescriptorAttempt::NotClaimed,
    }
}

/// A color word or `colorless` opens a description: no catalog identity
/// starts with a lowercase color adjective.
fn descriptor_color_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("colorless") || card_color_from_oracle_word(raw).is_some()
    })
}

pub(in crate::oracle::canonical) fn recognize_token_descriptor(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ExplicitTokenTemplateNode> {
    recognize_token_descriptor_in_context(cursor, true)
}

/// The descriptor of the older creature-token parents, which represent only
/// a literal power and toughness.
pub(in crate::oracle::canonical) fn recognize_creature_token_descriptor(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ExplicitTokenTemplateNode> {
    let node = recognize_token_descriptor_in_context(cursor, false)?;
    cursor.enter("token.descriptor.literal-statistics");
    cursor.require(
        node.literal_statistics(),
        "a literal power and toughness for this token parent",
    )?;
    Ok(node)
}

/// One subtype of a generic description: a creature subtype with lexical
/// CR 205.3m evidence, or a capitalized noncreature subtype (CR 205.3).
/// Returns the IR name, the creature subtype when it is one, and the width.
fn descriptor_subtype_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<(&'static str, Option<CreatureSubtype>, usize)> {
    let first = cursor.peek()?;
    if let Some((subtype, width)) =
        lookup_creature_subtype_tokens(first, cursor.peek_ahead(1), AcceptedForms::SingularOnly)
        && cursor
            .token_slice(cursor.position(), cursor.position() + width)
            .iter()
            .all(|token| token.token_type == "subtypeWord")
    {
        return Some((subtype.ir_name(), Some(subtype), width));
    }
    lookup_noncreature_subtype_forms(first, AcceptedForms::SingularOnly)
        .map(|subtype| (subtype, None, 1))
}

/// Noncreature subtypes that carry a printed power and toughness without
/// the creature type (CR 301.7, 301.7b).
const NONCREATURE_STATISTICS_SUBTYPES: [&str; 2] = ["Vehicle", "Spacecraft"];

fn recognize_token_descriptor_in_context(
    cursor: &mut OracleTokenCursor<'_>,
    generic: bool,
) -> MachineResult<ExplicitTokenTemplateNode> {
    let mut state = DescriptorState::Statistics;
    let mut statistics = None;
    let mut colors = Vec::new();
    let mut creature_subtypes = Vec::new();
    let mut subtypes = Vec::new();
    let mut types = Vec::new();
    loop {
        state = match state {
            DescriptorState::Statistics => {
                let stated = cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "counterValue")
                    || numeric_power_toughness_starts(cursor);
                if !stated && generic {
                    DescriptorState::Colors
                } else {
                    let literal_index = cursor.position();
                    let value = recognize_numeric_power_toughness(cursor)?;
                    cursor.enter("token.descriptor.statistics-range");
                    if let Some(literal) = value.literal()
                        && (i32::try_from(literal.power).is_err()
                            || i32::try_from(literal.toughness).is_err())
                    {
                        let token = &cursor.token_slice(literal_index, literal_index + 1)[0];
                        return Err(cursor.fail_with_span(
                            "token power and toughness representable as i32",
                            literal_index,
                            token.span_start,
                            token.span_end,
                            token.raw.clone(),
                        ));
                    }
                    statistics = Some(value);
                    DescriptorState::Colors
                }
            }
            DescriptorState::Colors => {
                cursor.enter("token.descriptor.colors");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("colorless"))
                {
                    cursor.consume("colorless")?;
                } else if generic && !descriptor_color_starts(cursor) {
                    // A description without a color word is colorless
                    // (`a Forest land token`, CR 105.2c).
                } else {
                    loop {
                        let color = cursor
                            .peek()
                            .and_then(card_color_from_oracle_word)
                            .ok_or_else(|| cursor.fail("a registered Magic color or colorless"))?;
                        cursor.require(!colors.contains(&color), "distinct token colors")?;
                        colors.push(color);
                        cursor.advance_to(cursor.position() + 1)?;
                        // A color-list connector is owned only when its next
                        // item is another color. Other connectors stay outside.
                        let checkpoint = cursor.checkpoint();
                        let has_separator = matches!(cursor.peek(), Some(","))
                            || cursor.peek().is_some_and(|s| s.eq_ignore_ascii_case("and"));
                        if !has_separator {
                            break;
                        }
                        if cursor.peek() == Some(",") {
                            cursor.consume(",")?;
                        }
                        if cursor.peek().is_some_and(|s| s.eq_ignore_ascii_case("and")) {
                            cursor.consume("and")?;
                        }
                        if cursor
                            .peek()
                            .and_then(card_color_from_oracle_word)
                            .is_none()
                        {
                            cursor.rollback(checkpoint);
                            break;
                        }
                    }
                }
                DescriptorState::Subtypes
            }
            DescriptorState::Subtypes => {
                if generic {
                    cursor.enter("token.descriptor.subtypes");
                    while !cursor.peek().is_some_and(|raw| {
                        lookup_card_type(raw, AcceptedForms::SingularOnly).is_some()
                    }) {
                        let (subtype, creature, width) =
                            descriptor_subtype_at(cursor).ok_or_else(|| {
                                cursor.fail("a registered creature or noncreature subtype")
                            })?;
                        cursor.require(!subtypes.contains(&subtype), "distinct token subtypes")?;
                        subtypes.push(subtype);
                        creature_subtypes.extend(creature);
                        cursor.advance_to(cursor.position() + width)?;
                    }
                } else {
                    creature_subtypes =
                        super::token_subtype_list::recognize_token_subtype_list(cursor)?.subtypes;
                    subtypes = creature_subtypes
                        .iter()
                        .map(|kind| kind.ir_name())
                        .collect();
                }
                DescriptorState::Types
            }
            DescriptorState::Types => {
                cursor.enter("token.descriptor.types");
                while let Some(kind) = cursor
                    .peek()
                    .and_then(|raw| lookup_card_type(raw, AcceptedForms::SingularOnly))
                {
                    cursor.enter("token.descriptor.permanent-type-domain");
                    cursor.require(
                        kind.may_appear_on_permanent(),
                        "card types compatible with battlefield presence (CR110.4)",
                    )?;
                    cursor.require(!types.contains(&kind), "distinct token card types")?;
                    types.push(kind);
                    cursor.advance_to(cursor.position() + 1)?;
                }
                cursor.enter("token.descriptor.types");
                cursor.require(!types.is_empty(), "a card type for this token descriptor")?;
                let creature = types.contains(&ObjectCardType::Creature);
                cursor.require(
                    creature
                        || creature_subtypes.is_empty()
                        || types.contains(&ObjectCardType::Kindred),
                    "the creature or kindred type for creature subtypes (CR 205.3d)",
                )?;
                if statistics.is_some() {
                    cursor.require(
                        creature
                            || subtypes
                                .iter()
                                .any(|subtype| NONCREATURE_STATISTICS_SUBTYPES.contains(subtype)),
                        "Creature among types for this power/toughness token descriptor",
                    )?;
                }
                DescriptorState::Complete
            }
            DescriptorState::Complete => {
                cursor.enter("token.descriptor.complete");
                // A variable or absent pair keeps zero placeholders in the
                // literal spec; `literal_statistics` guards every reader.
                let literal = statistics
                    .as_ref()
                    .and_then(BasePowerToughnessNode::literal)
                    .unwrap_or(PowerToughnessNode {
                        power: 0,
                        toughness: 0,
                    });
                return Ok(ExplicitTokenTemplateNode {
                    creature: CreatureTokenSpecNode {
                        power: literal.power,
                        toughness: literal.toughness,
                        colors,
                        subtypes: creature_subtypes,
                    },
                    types,
                    statistics,
                    subtypes,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "token_entry_state_tests.rs"]
mod entry_state_tests;

#[cfg(test)]
#[path = "token_descriptor_tests.rs"]
mod descriptor_tests;
