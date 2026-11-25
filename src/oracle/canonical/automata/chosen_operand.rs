//! A non-target indefinite operand of an ordinary action: `put a +1/+1
//! counter on a creature you control`, `attach it to a creature you
//! control`, `transform an Incubator token you control`.
//!
//! The written object is neither declared as a target (CR 115.1, 601.2c)
//! nor every member of a collection: the acting player chooses it while the
//! action resolves (CR 608.2d), or the game picks it `at random`. The node is
//! the same resolution choice `choose a creature you control` reads
//! (`choose_instruction::ObjectChoiceNode`): the program lowering emits that
//! choice right before the action and publishes the chosen object as the
//! action's own result binding, so a later `it` / `that creature` names it
//! (`subject_after_operand`). Nothing here is specific to a verb.
//!
//! ```text
//! Operand  := Count Query ["chosen"] ["at" "random"]
//! Count    := "a" | "an" | "another" | N | "up" "to" N     (never before `target`)
//! Query    := the shared battlefield-permanent object query
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ExpressedTargetCardinality, ObjectChoiceNode, PermanentEffectReference, TargetSelector,
    ordinal_target_head_follows, recognize_expressed_target_cardinality, recognize_object_query,
    resolution_choice_binding, resolution_selection_count_starts,
};

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// Lexical lookahead: an indefinite count that heads no `target` (`a
/// creature`, `an Incubator token`, `another creature`, `two lands`, `up to
/// one creature`). Dispatch hint only; the reader below owns the phrase.
pub(in crate::oracle::canonical) fn chosen_permanent_operand_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let article =
        word_at(cursor, 0, "a") || word_at(cursor, 0, "an") || word_at(cursor, 0, "another");
    if article {
        // `a number of`, `a single`: not an object determiner here; `another
        // target` and `a second target` are declarations.
        return !(word_at(cursor, 1, "target")
            || ordinal_target_head_follows(cursor)
            || word_at(cursor, 1, "number")
            || word_at(cursor, 1, "single")
            || word_at(cursor, 1, "player")
            || word_at(cursor, 1, "opponent"));
    }
    resolution_selection_count_starts(cursor)
}

/// `COUNT QUERY [chosen] [at random]`: the permanents the acting player
/// chooses on resolution (CR 608.2d), as the shared resolution choice. The
/// parent owns everything after the query.
pub(in crate::oracle::canonical) fn recognize_chosen_permanent_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectChoiceNode> {
    cursor.enter("chosen-operand.count");
    let mut another = false;
    let quantity = if word_at(cursor, 0, "a") || word_at(cursor, 0, "an") {
        let article = cursor.peek().unwrap_or("a").to_string();
        cursor.consume(&article)?;
        ExpressedTargetCardinality::EXACTLY_ONE
    } else if word_at(cursor, 0, "another") {
        // `another creature you control`: other than the ability's source.
        cursor.consume("another")?;
        another = true;
        ExpressedTargetCardinality::EXACTLY_ONE
    } else {
        recognize_expressed_target_cardinality(cursor)?
    };
    cursor.require(
        !word_at(cursor, 0, "target") && !word_at(cursor, 0, "other"),
        "a resolution choice; a `target` head is a declaration read by the target list",
    )?;
    cursor.enter("chosen-operand.query");
    let query_start = cursor.position();
    let mut query = recognize_object_query(cursor)?;
    // An article inside the query heads another object noun (`an artifact,
    // a creature, or a land`): one choice per head, not one operand.
    let query_tokens = cursor.token_slice(query_start, cursor.position());
    let repeated_head = query_tokens.iter().enumerate().any(|(index, token)| {
        (token.raw.eq_ignore_ascii_case("a") || token.raw.eq_ignore_ascii_case("an"))
            && !query_tokens.get(index + 1).is_some_and(|next| {
                ["opponent", "player", "single"]
                    .iter()
                    .any(|word| next.raw.eq_ignore_ascii_case(word))
            })
    });
    cursor.require(
        !repeated_head,
        "one counted query; a list of articled heads is one choice per head",
    )?;
    cursor.enter("chosen-operand.domain");
    cursor.require(
        query.denotes_battlefield_permanents(),
        "permanents on the battlefield chosen while the action resolves",
    )?;
    cursor.require(
        !(another && query.exclude_source),
        "one source exclusion (`another` or `other`)",
    )?;
    query.exclude_source |= another;
    // `chosen at random` / `at random` (CR 608.2d): the game picks among the
    // same candidates; only an exact count is picked at random.
    cursor.enter("chosen-operand.selection");
    let chosen =
        word_at(cursor, 0, "chosen") && word_at(cursor, 1, "at") && word_at(cursor, 2, "random");
    let random = chosen || (word_at(cursor, 0, "at") && word_at(cursor, 1, "random"));
    if random {
        cursor.require(
            quantity.minimum == quantity.maximum,
            "an exact number of permanents picked at random",
        )?;
        if chosen {
            cursor.consume("chosen")?;
        }
        cursor.consume_phrase(&["at", "random"])?;
    }
    let selector = match query.controller {
        Some(controller) => TargetSelector::ControlledObject {
            predicate: query.predicate.clone(),
            controller,
        },
        None => TargetSelector::Object(query.predicate.clone()),
    };
    cursor.enter("chosen-operand.complete");
    Ok(ObjectChoiceNode {
        quantity,
        query,
        selector,
        remainder: None,
        random,
    })
}

/// The reference the action at `effect_index` publishes for its chosen
/// operand: the one chosen object (`it`, `that creature`) or, for a choice
/// of several, the chosen set (`those creatures`). The program lowering binds
/// both keys to the choice's result binding.
pub(in crate::oracle::canonical) fn chosen_operand_reference(
    choice: &ObjectChoiceNode,
    effect_index: usize,
) -> PermanentEffectReference {
    let key = resolution_choice_binding(effect_index);
    if choice.singular() {
        PermanentEffectReference::Target(key)
    } else {
        PermanentEffectReference::TargetSet(key)
    }
}

#[cfg(test)]
#[path = "chosen_operand_tests.rs"]
mod tests;
