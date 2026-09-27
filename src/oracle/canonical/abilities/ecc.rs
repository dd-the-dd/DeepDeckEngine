use super::super::*;

fn ecc_effect(ability: &str) -> Value {
    json!({
        "kind": "resolveTriggeredInstruction",
        "operation": "eccRemainingAbility",
        "ability": ability,
    })
}

fn ecc_spell(ability: &str) -> CanonicalRuleDraft {
    draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [ecc_effect(ability)],
        }),
        &["Resolve the set-specific spell instruction"],
    )
}

fn ecc_trigger(event: Value, ability: &str) -> CanonicalRuleDraft {
    draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": [ecc_effect(ability)],
        }),
        &[
            "Recognize the triggering event",
            "Resolve the set-specific instruction",
        ],
    )
}

fn ecc_activated(costs: Value, ability: &str, sorcery_only: bool) -> CanonicalRuleDraft {
    let mut rule = json!({
        "kind": "activatedAbility",
        "source": self_ref(),
        "costs": costs,
        "effects": [ecc_effect(ability)],
    });
    if sorcery_only {
        rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
    }
    draft(
        rule,
        &[
            "Resolve activation costs",
            "Resolve the set-specific instruction",
        ],
    )
}

fn ecc_static(ability: &str) -> CanonicalRuleDraft {
    draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "eccStaticAbility",
                "ability": ability,
                "player": controller(),
            }],
        }),
        &["Install the set-specific continuous rule"],
    )
}

fn self_enters() -> Value {
    json!({ "kind": "enterBattlefield", "object": self_ref() })
}

fn self_dies() -> Value {
    json!({ "kind": "permanentDied", "object": self_ref() })
}

pub(in crate::oracle::canonical) fn parse_ecc_remaining_ability(
    text: &str,
    _ability_kind: &str,
) -> Option<CanonicalRuleDraft> {
    if text.starts_with("Elemental permanent spells you cast from your hand gain evoke {4}") {
        return Some(ecc_static("ashlingGrantEvoke"));
    }
    if text.starts_with(
        "Whenever you sacrifice a nontoken Elemental, create a token that's a copy of it",
    ) {
        return Some(ecc_trigger(
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": and(vec![subtype("Elemental"), not(json!({ "kind": "isToken" }))]),
                "reason": "sacrificed",
            }),
            "ashlingSacrificedElemental",
        ));
    }
    if text.starts_with("When this creature enters, you may cast target instant or sorcery card from a graveyard without paying its mana cost") {
        return Some(ecc_trigger(self_enters(), "impulsivityCastGraveyardSpell"));
    }
    if text.starts_with("Whenever a creature you control with a counter on it dies, you may return another target permanent card") {
        return Some(ecc_trigger(
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": card_type("Creature"),
                "hadAnyCounters": true,
            }),
            "pucasCovenantReturn",
        ));
    }
    if text.starts_with("You lose 2 life and draw two cards, then clash with an opponent") {
        return Some(ecc_spell("hoardersGreed"));
    }
    if text.starts_with("When this creature enters, it deals 4 damage divided as you choose") {
        return Some(ecc_trigger(self_enters(), "furyDividedDamage"));
    }
    if text.starts_with("Fire Covenant deals X damage divided as you choose") {
        return Some(ecc_spell("fireCovenantDividedDamage"));
    }
    if text
        == "Each player who controls a creature with power 4 or greater draws a card. Then destroy all creatures."
    {
        return Some(ecc_spell("shatterTheSky"));
    }
    if text.starts_with(
        "Exile target creature. Its controller manifests the top card of their library.",
    ) {
        return Some(ecc_spell("realityShift"));
    }
    if text
        == "Whenever you cycle or discard another card, put a -1/-1 counter on each creature your opponents control."
    {
        return Some(ecc_trigger(
            json!({ "kind": "playerDiscardedCard", "player": controller() }),
            "archfiendMinusCounters",
        ));
    }
    if text.starts_with(
        "Choose a creature type. Return up to two creature cards of that type from your graveyard",
    ) {
        return Some(ecc_spell("hauntingVoyage"));
    }
    if text.contains(": Destroy target creature with a -1/-1 counter on it.") {
        return Some(ecc_activated(
            json!([{ "kind": "payLoyalty", "object": self_ref(), "amount": integer(-3) }]),
            "lilianaDestroyMinusCounterCreature",
            true,
        ));
    }
    if text.starts_with("Compleated (") {
        return Some(ecc_static("vraskaCompleated"));
    }
    if text.contains(": Target creature becomes a Treasure artifact with") {
        return Some(ecc_activated(
            json!([{ "kind": "payLoyalty", "object": self_ref(), "amount": integer(-2) }]),
            "vraskaTreasureCreature",
            true,
        ));
    }
    if text.contains(": If target player has fewer than nine poison counters") {
        return Some(ecc_activated(
            json!([{ "kind": "payLoyalty", "object": self_ref(), "amount": integer(-9) }]),
            "vraskaSetNinePoison",
            true,
        ));
    }
    if text.starts_with("Choose one") && text.contains("Cathartic Pyre deals 3 damage") {
        return Some(ecc_spell("catharticPyre"));
    }
    if text.starts_with("Whenever one or more creatures you control deal combat damage to a player")
    {
        return Some(ecc_trigger(
            json!({ "kind": "controlledCreaturesCombatDamageToPlayer", "player": controller() }),
            "descendantsFury",
        ));
    }
    if text.starts_with(
        "{1}{R}, {T}: You may put an Elemental creature card from your hand onto the battlefield",
    ) {
        return Some(ecc_activated(
            json!([
                { "kind": "payMana", "manaCost": "{1}{R}" },
                { "kind": "tap", "object": self_ref() }
            ]),
            "incandescentSoulstoke",
            false,
        ));
    }
    if text.starts_with(
        "When this creature enters, reveal the top five cards of your library. Put a land card",
    ) {
        return Some(ecc_trigger(self_enters(), "cavalierRevealLand"));
    }
    if text.starts_with("When this creature dies, you may exile it. If you do, put another target card from your graveyard on top") {
        return Some(ecc_trigger(self_dies(), "cavalierDeathTopCard"));
    }
    if text.starts_with(
        "Whenever a creature you control enters, you may look at the top X cards of your library",
    ) {
        return Some(ecc_trigger(
            json!({ "kind": "permanentEntered", "player": controller(), "where": card_type("Creature") }),
            "creamOfTheCrop",
        ));
    }
    if text.starts_with(
        "Whenever another creature enters, its controller may draw a card if its power is greater",
    ) {
        return Some(ecc_trigger(
            json!({ "kind": "permanentEntered", "anyController": true, "where": card_type("Creature"), "excludeSource": true }),
            "selvalaGreaterPowerDraw",
        ));
    }
    if text.starts_with(
        "{G}, {T}: Add X mana in any combination of colors, where X is the greatest power",
    ) {
        return Some(ecc_activated(
            json!([
                { "kind": "payMana", "manaCost": "{G}" },
                { "kind": "tap", "object": self_ref() }
            ]),
            "selvalaGreatestPowerMana",
            false,
        ));
    }
    if text.starts_with("When this creature enters, choose two")
        && text.contains("Create a 4/4 green Rhino Warrior")
    {
        return Some(ecc_trigger(self_enters(), "titanOfIndustry"));
    }
    if text == "Damage can't be prevented." {
        return Some(ecc_static("damageCannotBePrevented"));
    }
    if text.starts_with("All damage is dealt as though its source had wither.") {
        return Some(ecc_static("allDamageHasWither"));
    }
    if text.starts_with("Whenever Glissa Sunslayer deals combat damage to a player, choose one") {
        return Some(ecc_trigger(
            json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
            "glissaSunslayer",
        ));
    }
    if text.starts_with("{W}{U}{B}{R}{G}: You may play target Elemental card from your graveyard") {
        return Some(ecc_activated(
            json!([{ "kind": "payMana", "manaCost": "{W}{U}{B}{R}{G}" }]),
            "hordeOfNotions",
            false,
        ));
    }
    if text.starts_with("Companion")
        && text.contains("No card in your starting deck has more than one of the same mana symbol")
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "companion",
                    "acquisitionCost": { "kind": "payMana", "manaCost": "{3}" },
                    "timing": { "kind": "sorceryTiming" },
                    "deckCondition": { "kind": "noRepeatedManaSymbol" },
                },
            }),
            &["Validate the companion starting-deck condition"],
        ));
    }
    if text == "Creatures your opponents control with counters on them can't attack or block." {
        return Some(ecc_static("kulrathCounteredCreaturesCantAttackOrBlock"));
    }
    if text.starts_with("During each of your turns, you may play a land and cast a permanent spell of each permanent type from your graveyard") {
        return Some(ecc_static("muldrothaGraveyardPermission"));
    }
    if text.starts_with("As this land enters, you may reveal an Elemental card from your hand") {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "revealedElemental",
                    "kind": "chooseRevealCard",
                    "where": subtype("Elemental"),
                    "minimum": integer(0),
                    "maximum": integer(1),
                }],
                "replacement": [{
                    "kind": "eccEntryAbility",
                    "ability": "elementalRevealLandEntry",
                }],
            }),
            &[
                "Choose whether to reveal an Elemental",
                "Set the entering land state",
            ],
        ));
    }
    if text.starts_with(
        "{1}, {T}: Move a counter from target permanent you control onto a second target permanent",
    ) {
        return Some(ecc_activated(
            json!([
                { "kind": "payMana", "manaCost": "{1}" },
                { "kind": "tap", "object": self_ref() }
            ]),
            "nestingGroundsMoveCounter",
            true,
        ));
    }
    if text.starts_with(
        "{2}{R}{G}: Until end of turn, this land becomes a 3/3 red and green Elemental creature",
    ) {
        return Some(ecc_activated(
            json!([{ "kind": "payMana", "manaCost": "{2}{R}{G}" }]),
            "ragingRavineAnimate",
            false,
        ));
    }
    if text.starts_with("This land enters tapped. As it enters, choose a color other than ") {
        let omitted = if text.ends_with("red.") {
            "R"
        } else if text.ends_with("green.") {
            "G"
        } else if text.ends_with("blue.") {
            "U"
        } else if text.ends_with("black.") {
            "B"
        } else {
            "W"
        };
        let options = ["W", "U", "B", "R", "G"]
            .into_iter()
            .filter(|color| *color != omitted)
            .collect::<Vec<_>>();
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "thrivingColor",
                    "kind": "chooseColor",
                    "options": options,
                }],
                "replacement": [
                    { "kind": "setEnteringState", "tapped": true },
                    { "kind": "storeDecision", "decisionId": "thrivingColor" },
                ],
            }),
            &["Enter tapped", "Choose and store a non-native color"],
        ));
    }
    None
}
