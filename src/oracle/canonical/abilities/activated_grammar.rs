use super::super::*;

pub(in crate::oracle::canonical) enum ActivationReduction<'a> {
    Conditional {
        instruction: &'a str,
        amount: i64,
        condition: &'a str,
    },
    PerControlledPermanent {
        instruction: &'a str,
        amount: i64,
        criteria: &'a str,
    },
}

pub(in crate::oracle::canonical) struct TokenReflexiveDamage<'a> {
    pub token_instruction: &'a str,
    pub required_permanent: &'a str,
    pub excluded_name: &'a str,
    pub counted_permanents: &'a str,
}

pub(in crate::oracle::canonical) struct LinkedPermanentExchange<'a> {
    pub battlefield_criteria: &'a str,
    pub graveyard_criteria: &'a str,
    pub sacrificed_criteria: &'a str,
    pub returned_criteria: &'a str,
}

pub(in crate::oracle::canonical) enum CounterRecipient<'a> {
    Source,
    EachControlled(&'a str),
    NamedSource,
}

pub(in crate::oracle::canonical) struct PutCounter<'a> {
    pub count: Value,
    pub counter: &'a str,
    pub recipient: CounterRecipient<'a>,
}

pub(in crate::oracle::canonical) struct BasicLandSearch<'a> {
    pub maximum: i64,
    pub description: &'a str,
    pub destination: &'static str,
    pub tapped: bool,
}

pub(in crate::oracle::canonical) fn parse_search_then_optional_behold_untap(
    instruction: &str,
) -> Option<&str> {
    let rest = strip_prefix_ascii_case(instruction, "Search your library for ")?;
    let rest = strip_prefix_ascii_case(rest, "a basic land card, ")?;
    let rest = strip_prefix_ascii_case(
        rest,
        "put it onto the battlefield tapped, then shuffle. You may behold ",
    )?;
    let (criteria, rest) = split_once_ascii_case(rest, ". If you do, ")?;
    let rest = strip_suffix_ascii_case(rest, ".").unwrap_or(rest);
    rest.eq_ignore_ascii_case("untap that land")
        .then(|| strip_leading_article(criteria))
}

pub(in crate::oracle::canonical) fn parse_activation_reduction(
    instruction: &str,
) -> Option<ActivationReduction<'_>> {
    let (instruction, reduction) = split_once_ascii_case(instruction, " This ability costs ")?;
    let reduction = strip_suffix_ascii_case(reduction.trim(), ".").unwrap_or(reduction.trim());
    let reduction = strip_prefix_ascii_case(reduction, "{")?;
    let (amount, clause) = reduction.split_once('}')?;
    let amount = parse_number_word(amount)?;
    if let Some(condition) = strip_prefix_ascii_case(clause, " less to activate if ") {
        return Some(ActivationReduction::Conditional {
            instruction,
            amount,
            condition,
        });
    }
    let criteria = strip_prefix_ascii_case(clause, " less to activate for each ")?;
    let criteria = strip_suffix_ascii_case(criteria, " you control")?;
    Some(ActivationReduction::PerControlledPermanent {
        instruction,
        amount,
        criteria,
    })
}

pub(in crate::oracle::canonical) fn parse_token_reflexive_count_damage(
    instruction: &str,
) -> Option<TokenReflexiveDamage<'_>> {
    let (token_instruction, rest) =
        split_once_ascii_case(instruction, " When you do, if you control ")?;
    let token_instruction = strip_suffix_ascii_case(token_instruction, ".")?;
    let (required_permanent, rest) = split_once_ascii_case(rest, " permanent other than ")?;
    let (excluded_name, rest) = rest.split_once(',')?;
    let rest = rest.trim_start();
    let rest = ["it ", "he ", "she ", "they "]
        .into_iter()
        .find_map(|pronoun| strip_prefix_ascii_case(rest, pronoun))?;
    let rest = strip_prefix_ascii_case(rest, "deals damage equal to the number of ")?;
    let counted_permanents = strip_suffix_ascii_case(rest, " you control to any target.")?;
    Some(TokenReflexiveDamage {
        token_instruction,
        required_permanent: strip_leading_article(required_permanent),
        excluded_name: excluded_name.trim(),
        counted_permanents,
    })
}

pub(in crate::oracle::canonical) fn parse_protected_attack_stat_change(
    instruction: &str,
) -> Option<(i64, i64)> {
    let stats = strip_prefix_ascii_case(
        instruction,
        "Until your next turn, whenever a creature attacks you or a planeswalker you control, it gets ",
    )?;
    let stats = strip_suffix_ascii_case(stats, " until end of turn.")?;
    let (power, toughness) = stats.split_once('/')?;
    Some((power.parse().ok()?, toughness.parse().ok()?))
}

pub(in crate::oracle::canonical) fn parse_optional_hand_permanent_with_haste_and_delayed_sacrifice(
    instruction: &str,
) -> Option<&str> {
    let rest = strip_prefix_ascii_case(instruction, "You may put ")?;
    let (criteria, rest) =
        split_once_ascii_case(rest, " card from your hand onto the battlefield. ")?;
    let (haste, sacrifice) = rest.split_once(". ")?;
    let haste_subject = strip_suffix_ascii_case(haste, " gains haste")?;
    if !matches!(
        haste_subject.to_ascii_lowercase().as_str(),
        "that creature" | "that permanent"
    ) {
        return None;
    }
    let sacrifice = strip_suffix_ascii_case(sacrifice, ".").unwrap_or(sacrifice);
    let sacrifice = strip_prefix_ascii_case(sacrifice, "Sacrifice ")?;
    let (subject, timing) = split_once_ascii_case(sacrifice, " at ")?;
    if !matches!(
        subject.to_ascii_lowercase().as_str(),
        "the creature" | "that creature" | "it"
    ) || !timing.eq_ignore_ascii_case("the beginning of the next end step")
    {
        return None;
    }
    Some(strip_leading_article(criteria))
}

pub(in crate::oracle::canonical) fn parse_sacrificed_power_draw_then_discard(
    instruction: &str,
) -> Option<&str> {
    let criteria = strip_prefix_ascii_case(instruction, "Draw cards equal to the sacrificed ")?;
    strip_suffix_ascii_case(criteria, "'s power, then discard a card.")
}

pub(in crate::oracle::canonical) fn parse_target_player_mill_sacrificed_power(
    instruction: &str,
) -> Option<&str> {
    let criteria = strip_prefix_ascii_case(
        instruction,
        "Target player mills cards equal to the sacrificed ",
    )?;
    strip_suffix_ascii_case(criteria, "'s power.")
}

pub(in crate::oracle::canonical) fn parse_single_graveyard_cast_permission(
    instruction: &str,
) -> Option<&str> {
    let criteria = strip_prefix_ascii_case(instruction, "Choose target ")?;
    let (criteria, rest) = split_once_ascii_case(criteria, " card in your graveyard. ")?;
    rest.eq_ignore_ascii_case(
        "If you haven't cast a spell this turn, you may cast that card. If you do, you can't cast additional spells this turn.",
    )
    .then_some(criteria)
}

pub(in crate::oracle::canonical) fn parse_linked_permanent_exchange(
    instruction: &str,
) -> Option<LinkedPermanentExchange<'_>> {
    let first = strip_prefix_ascii_case(instruction, "Choose target ")?;
    let (first, second) = split_once_ascii_case(
        first,
        ". If both targets are still legal as this ability resolves, ",
    )?;
    let (battlefield_criteria, graveyard_criteria) =
        split_once_ascii_case(first, " a player controls and target ")?;
    let graveyard_criteria =
        strip_suffix_ascii_case(graveyard_criteria, " card in that player's graveyard")?;
    let second = strip_prefix_ascii_case(second, "that player simultaneously sacrifices the ")?;
    let (sacrificed_criteria, returned_criteria) =
        split_once_ascii_case(second, " and returns the ")?;
    let returned_criteria =
        strip_suffix_ascii_case(returned_criteria, " card to the battlefield.")?;
    Some(LinkedPermanentExchange {
        battlefield_criteria,
        graveyard_criteria,
        sacrificed_criteria,
        returned_criteria,
    })
}

pub(in crate::oracle::canonical) fn parse_counted_draw(instruction: &str) -> Option<Value> {
    let count = strip_prefix_ascii_case(instruction, "Draw ")?;
    let count = strip_suffix_ascii_case(count, " cards.")
        .or_else(|| strip_suffix_ascii_case(count, " card."))?;
    parse_numeric_expression_text(count)
}

pub(in crate::oracle::canonical) fn parse_put_counter(instruction: &str) -> Option<PutCounter<'_>> {
    let instruction = strip_suffix_ascii_case(instruction.trim(), ".")?;
    let instruction = strip_prefix_ascii_case(instruction, "Put ")?;
    let (quantity_and_counter, recipient) = split_once_ascii_case(instruction, " counter on ")?;
    let (count, counter) = parse_quantity_prefix(quantity_and_counter)?;
    let recipient = if strip_prefix_ascii_case(recipient, "this ").is_some() {
        CounterRecipient::Source
    } else if let Some(criteria) = strip_prefix_ascii_case(recipient, "each ")
        .and_then(|criteria| strip_suffix_ascii_case(criteria, " you control"))
    {
        CounterRecipient::EachControlled(criteria)
    } else {
        CounterRecipient::NamedSource
    };
    Some(PutCounter {
        count,
        counter: counter.trim(),
        recipient,
    })
}

pub(in crate::oracle::canonical) fn parse_remove_counter(
    instruction: &str,
) -> Option<(Value, &str)> {
    let instruction = strip_suffix_ascii_case(instruction.trim(), ".")?;
    let instruction = strip_prefix_ascii_case(instruction, "Remove ")?;
    let (quantity_and_counter, _) = split_once_ascii_case(instruction, " counter from ")?;
    let (count, counter) = parse_quantity_prefix(quantity_and_counter)?;
    Some((count, counter))
}

pub(in crate::oracle::canonical) fn parse_you_gain_life(instruction: &str) -> Option<Value> {
    let amount = strip_prefix_ascii_case(instruction, "You gain ")?;
    let amount = strip_suffix_ascii_case(amount, " life.")?;
    parse_numeric_expression_text(amount)
}

pub(in crate::oracle::canonical) fn parse_source_damage(
    instruction: &str,
) -> Option<(Value, &str, Option<&str>)> {
    let instruction = ["This creature deals ", "It deals "]
        .into_iter()
        .find_map(|prefix| strip_prefix_ascii_case(instruction, prefix))?;
    let (damage, followup) = instruction
        .split_once(". ")
        .map(|(damage, followup)| (damage, Some(followup)))
        .unwrap_or((instruction, None));
    let damage = strip_suffix_ascii_case(damage, ".").unwrap_or(damage);
    let (amount, recipient) = split_once_ascii_case(damage, " damage to ")?;
    Some((parse_numeric_expression_text(amount)?, recipient, followup))
}

pub(in crate::oracle::canonical) fn parse_temporary_unblockable_target(
    instruction: &str,
) -> Option<&str> {
    let criteria = strip_prefix_ascii_case(instruction, "Target ")?;
    strip_suffix_ascii_case(criteria, " can't be blocked this turn.")
}

pub(in crate::oracle::canonical) fn parse_destroy_target(instruction: &str) -> Option<&str> {
    let criteria = strip_prefix_ascii_case(instruction, "Destroy target ")?;
    strip_suffix_ascii_case(criteria, ".")
}

pub(in crate::oracle::canonical) fn parse_copy_target_retaining_ability(
    instruction: &str,
) -> Option<&str> {
    let instruction = strip_prefix_ascii_case(instruction, "This ")?;
    let (_, target) = split_once_ascii_case(instruction, " becomes a copy of target ")?;
    strip_suffix_ascii_case(target, ", except it has this ability.")
}

pub(in crate::oracle::canonical) fn parse_basic_land_search(
    instruction: &str,
) -> Option<BasicLandSearch<'_>> {
    let instruction = strip_suffix_ascii_case(instruction.trim(), ".").unwrap_or(instruction);
    let instruction = strip_prefix_ascii_case(instruction, "Search your library for ")?;
    let (selection, destination) = split_once_ascii_case(instruction, " card,")
        .or_else(|| split_once_ascii_case(instruction, " cards,"))?;
    let (maximum, description) =
        if let Some(selection) = strip_prefix_ascii_case(selection.trim(), "up to ") {
            let (count, description) = selection.split_once(' ')?;
            (parse_number_word(count)?, description)
        } else {
            let (count, description) = selection.split_once(' ')?;
            (parse_number_word(count)?, description)
        };
    if !description.to_ascii_lowercase().starts_with("basic ")
        && !description.eq_ignore_ascii_case("basic land")
    {
        return None;
    }
    let destination = destination.trim_start();
    let destination = ["reveal it, ", "reveal them, "]
        .into_iter()
        .find_map(|prefix| strip_prefix_ascii_case(destination, prefix))
        .unwrap_or(destination);
    let destination = ["put it ", "put them "]
        .into_iter()
        .find_map(|prefix| strip_prefix_ascii_case(destination, prefix))?;
    let destination = strip_suffix_ascii_case(destination, ", then shuffle")?;
    let (destination, tapped) = if destination.eq_ignore_ascii_case("onto the battlefield tapped") {
        ("battlefield", true)
    } else if destination.eq_ignore_ascii_case("onto the battlefield") {
        ("battlefield", false)
    } else if destination.eq_ignore_ascii_case("into your hand") {
        ("hand", false)
    } else {
        return None;
    };
    Some(BasicLandSearch {
        maximum,
        description,
        destination,
        tapped,
    })
}

pub(in crate::oracle::canonical) fn parse_station_threshold(text: &str) -> Option<(i64, &str)> {
    let (threshold, ability) = text.split_once("+ | ")?;
    Some((parse_number_word(threshold)?, ability))
}

pub(in crate::oracle::canonical) fn parse_class_level(text: &str) -> Option<(&str, i64)> {
    let (cost, level) = split_once_ascii_case(text, ": Level ")?;
    let level = parse_number_word(level)?;
    (level >= 2).then_some((cost, level))
}

pub(in crate::oracle::canonical) fn parse_library_search_to_battlefield(
    instruction: &str,
) -> Option<(&str, bool)> {
    let instruction = strip_prefix_ascii_case(instruction, "Search your library for ")?;
    let (description, destination) = split_once_ascii_case(instruction, " card, put it ")?;
    let destination = strip_suffix_ascii_case(destination, ", then shuffle.")?;
    let tapped = if destination.eq_ignore_ascii_case("onto the battlefield tapped") {
        true
    } else if destination.eq_ignore_ascii_case("onto the battlefield") {
        false
    } else {
        return None;
    };
    Some((description, tapped))
}

pub(in crate::oracle::canonical) fn parse_random_graveyard_card_return(
    instruction: &str,
) -> Option<&str> {
    let instruction = strip_prefix_ascii_case(instruction, "Return ")?;
    let criteria = strip_suffix_ascii_case(
        instruction,
        " card at random from your graveyard to your hand.",
    )?;
    Some(strip_leading_article(criteria))
}
