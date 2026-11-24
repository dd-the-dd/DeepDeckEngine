//! The copiable-value exceptions of a copy (CR 707.9b, 707.10).
//!
//! ```text
//! Exceptions := "," "except" ["that"] Item ("and" Item)*
//! Item       := [Subject] ("is"|"are") Complement | Subject's Complement
//!             | [Subject] ("has"|"have") GrantedList
//!             | [Subject] ("isn't"|"aren't") "legendary"
//! Subject    := "it" | "they" | "the" ("copy"|"copies"|"token"|"tokens")
//! Complement := "not" "legendary" | [Article] [PT] [Colors] [TypeNouns
//!               "in" "addition" "to" ("its"|"their") "other" "types"]
//! ```
//!
//! Every copy reads its exceptions here: a token copy, a permanent entering
//! as a copy and a copy of a spell or ability on the stack. The
//! characteristics are the shared characteristic-change frame's children;
//! the result is the typed modification list each copy lowers.
use super::super::OracleTokenCursor;
use super::*;

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The copiable-value exceptions of a copy (CR 707.9b). The characteristics
/// are the ones of the shared characteristic-change frame; `isn't
/// legendary` removes a supertype, which no `becomes` complement states.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TokenCopyExceptionsNode {
    pub changes: Option<BecomeTypesNode<()>>,
    pub not_legendary: bool,
}

/// The start of one exception item: a subject (with or without its
/// contracted copula), or a verb sharing the previous item's subject.
fn exception_item_starts(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    copy_subject_width(cursor, offset).is_some()
        || [
            "it's", "they're", "has", "have", "isn't", "aren't", "is", "are",
        ]
        .iter()
        .any(|word| word_at(cursor, offset, word))
}

/// The subject of an exception item: the object being made (`it`, `they`)
/// or its definite description (`the copy`, `the copies`, `the token`,
/// `the tokens`, CR 707.9b). Returns its token width.
fn copy_subject_width(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    if word_at(cursor, offset, "it") || word_at(cursor, offset, "they") {
        return Some(1);
    }
    (word_at(cursor, offset, "the")
        && ["copy", "copies", "token", "tokens"]
            .iter()
            .any(|noun| word_at(cursor, offset + 1, noun)))
    .then_some(2)
}

/// `, except it's 1/1 and it's a Nightmare in addition to its other types`,
/// `, except it has haste`, `, except they aren't legendary`, `, except that
/// the copy is red`. Each item is one copiable-value exception (CR 707.9b);
/// items are coordinated by `and`. The one reader of every copy: token
/// copies, entering copies (`entry_frame.rs`) and stack copies
/// (`stack_copy.rs`). An item it cannot read is an explicit failure.
pub(in crate::oracle::canonical) fn recognize_copy_exceptions(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> super::super::MachineResult<TokenCopyExceptionsNode> {
    cursor.enter("token.copy.except");
    cursor.consume_phrase(&[",", "except"])?;
    // `except that the copy is red`: the complementizer before a subject.
    if word_at(cursor, 0, "that") && copy_subject_width(cursor, 1).is_some() {
        cursor.consume("that")?;
    }
    let mut exceptions = TokenCopyExceptionsNode::default();
    let mut changes = BecomeTypesNode::empty(());
    let number = if plural {
        TypeChangeNumber::Plural
    } else {
        TypeChangeNumber::Singular
    };
    let mut first = true;
    loop {
        cursor.enter("token.copy.except.item");
        let copula = if word_at(cursor, 0, "it's") || word_at(cursor, 0, "they're") {
            let raw = cursor.peek().expect("a contracted copula").to_string();
            cursor.consume(&raw)?;
            true
        } else {
            if let Some(width) = copy_subject_width(cursor, 0) {
                cursor.advance_to(cursor.position() + width)?;
            } else {
                cursor.require(!first, "a subject for the first copy exception")?;
            }
            false
        };
        first = false;
        if copula {
            recognize_copula_exception(cursor, number, &mut changes, &mut exceptions)?;
        } else if word_at(cursor, 0, "has") || word_at(cursor, 0, "have") {
            let raw = cursor.peek().expect("a possession verb").to_string();
            cursor.consume(&raw)?;
            let list =
                recognize_effect_owned_granted_ability_list(cursor, GRANTED_OBJECT_TYPE_LINE)?;
            changes.characteristics.keywords.extend(list.keywords);
            changes.characteristics.abilities.extend(list.abilities);
        } else if word_at(cursor, 0, "isn't") || word_at(cursor, 0, "aren't") {
            let raw = cursor.peek().expect("a negated copula").to_string();
            cursor.consume(&raw)?;
            cursor.consume("legendary")?;
            cursor.require(!exceptions.not_legendary, "one `isn't legendary` exception")?;
            exceptions.not_legendary = true;
        } else {
            cursor.enter("token.copy.except.copula");
            let raw = cursor
                .peek()
                .map(str::to_ascii_lowercase)
                .ok_or_else(|| cursor.fail("a copy exception"))?;
            cursor.require(
                matches!(raw.as_str(), "is" | "are"),
                "a copy exception verb",
            )?;
            cursor.consume(&raw)?;
            recognize_copula_exception(cursor, number, &mut changes, &mut exceptions)?;
        }
        cursor.enter("token.copy.except.separator");
        let mut next = cursor.clone();
        if next.peek() == Some(",") {
            next.consume(",")?;
        }
        if word_at(&next, 0, "and") && exception_item_starts(&next, 1) {
            next.consume("and")?;
            *cursor = next;
            continue;
        }
        break;
    }
    let states_changes = changes.names_types_or_characteristics();
    exceptions.changes = states_changes.then_some(changes);
    cursor.require(
        exceptions.changes.is_some() || exceptions.not_legendary,
        "at least one copy exception",
    )?;
    Ok(exceptions)
}

/// The complement of a copula in an exception: `not legendary`, a base
/// power/toughness, colors, or type nouns in addition to the copy's types.
fn recognize_copula_exception(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
    changes: &mut BecomeTypesNode<()>,
    exceptions: &mut TokenCopyExceptionsNode,
) -> super::super::MachineResult<()> {
    cursor.enter("token.copy.except.complement");
    if word_at(cursor, 0, "not") && word_at(cursor, 1, "legendary") {
        cursor.consume_phrase(&["not", "legendary"])?;
        cursor.require(!exceptions.not_legendary, "one `not legendary` exception")?;
        exceptions.not_legendary = true;
        return Ok(());
    }
    let article = word_at(cursor, 0, "a") || word_at(cursor, 0, "an");
    if article {
        let raw = cursor.peek().expect("an article").to_string();
        cursor.consume(&raw)?;
    }
    if numeric_power_toughness_starts(cursor) {
        cursor.require(
            changes.base_power_toughness().is_none(),
            "one base power/toughness exception",
        )?;
        changes.power_toughness = Some(recognize_numeric_power_toughness(cursor)?);
    }
    if cursor
        .peek()
        .and_then(crate::oracle::characteristic_vocabulary::lookup_card_color)
        .is_some()
        || word_at(cursor, 0, "colorless")
    {
        cursor.require(changes.colors.is_none(), "one color exception")?;
        changes.colors = Some(recognize_color_complement(cursor)?);
    }
    let nouns_number = if article {
        TypeChangeNumber::Singular
    } else {
        number
    };
    if type_change_noun_at(cursor, 0, nouns_number) {
        cursor.require(
            changes.subtypes.is_empty() && changes.card_types.is_empty(),
            "one type exception",
        )?;
        let nouns = recognize_type_change_nouns(cursor, nouns_number)?;
        cursor.enter("token.copy.except.retention");
        // A copy exception that adds types keeps the copied ones only when
        // the text says so; a replacing type exception has no primitive.
        cursor.consume_phrase(&["in", "addition", "to"])?;
        cursor.consume(match number {
            TypeChangeNumber::Singular => "its",
            TypeChangeNumber::Plural => "their",
        })?;
        cursor.consume_phrase(&["other", "types"])?;
        changes.subtypes = nouns.subtypes;
        changes.card_types = nouns.card_types;
        changes.retention = TypeChangeRetention::InAdditionToOtherTypes;
    } else {
        cursor.require(
            changes.power_toughness.is_some() || changes.colors.is_some(),
            "a power/toughness, color or type exception after the copula",
        )?;
        cursor.require(!article, "type nouns after `a` or `an`")?;
    }
    Ok(())
}

impl<Subject> BecomeTypesNode<Subject> {
    /// Whether any copiable characteristic is stated.
    fn names_types_or_characteristics(&self) -> bool {
        !self.subtypes.is_empty()
            || !self.card_types.is_empty()
            || self.colors.is_some()
            || self.base_power_toughness().is_some()
            || !self.characteristics.keywords.is_empty()
            || !self.characteristics.abilities.is_empty()
    }
}
