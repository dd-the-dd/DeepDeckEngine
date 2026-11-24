use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Words that end an `Equip [quality]` claim: a sentence about equip
/// abilities (`Equip abilities you activate ... cost {1} less`) is a static
/// ability, not the keyword.
const NON_QUALITY_WORDS: &[&str] = &["abilities", "ability", "you", "that", "cost", "costs"];

pub(in crate::oracle::canonical) fn claims_promoted_mana_equip_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    if !tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("equip"))
    {
        return false;
    }
    let rest = &tokens[1..];
    // `Equip {cost}`, `Equip—COSTS`, or the CR 702.6c `Equip [quality]
    // [creature] {cost}` form whose short quality precedes the mana cost.
    rest.first()
        .is_some_and(|token| token.token_type == "manaSymbol" || token.raw == "\u{2014}")
        || rest
            .iter()
            .position(|token| token.token_type == "manaSymbol")
            .is_some_and(|cost| {
                (1..=3).contains(&cost)
                    && rest[..cost].iter().all(|token| {
                        token
                            .raw
                            .chars()
                            .all(|character| character.is_alphanumeric() || character == '-')
                            && !NON_QUALITY_WORDS
                                .iter()
                                .any(|word| token.raw.eq_ignore_ascii_case(word))
                    })
            })
}

pub(in crate::oracle::canonical) fn parse_promoted_mana_equip(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-equip", input.tokens);
    let equip = recognize_mana_equip(&mut cursor)?;
    // The attach runtime pays a mana cost or one sacrificed permanent; any
    // other printed payment fails here instead of being accepted unpaid.
    cursor.enter("keyword.equip.cost-support");
    let [payment] = equip.costs.costs.as_slice() else {
        return Err(cursor.fail(
            "one Equip payment the attach runtime pays: a mana cost or one sacrificed permanent",
        ));
    };
    let cost = lower_keyword_cost_payment(&cursor, payment)?;
    cursor.require(
        matches!(
            cost["kind"].as_str(),
            Some("payMana" | "sacrificePermanent")
        ),
        "one Equip payment the attach runtime pays: a mana cost or one sacrificed permanent",
    )?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut ability = json!({
        "kind": "equip",
        "costs": [cost],
    });
    if let Some(quality) = &equip.quality {
        ability["where"] = lower_permanent_object_predicate(&ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            quality.clone(),
        ]));
    }
    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": ability,
        }),
        &[
            "Recognize Equip independently of any card name",
            "Read the printed activation cost with the shared mana-cost or cost-sequence child",
            "Validate the optional attach-and-sorcery reminder structurally",
            "Lower to the existing primitive equip keyword contract",
            &trace,
        ],
    ))
}
