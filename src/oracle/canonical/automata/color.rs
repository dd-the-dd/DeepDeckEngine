use super::CardColor;
use crate::oracle::characteristic_vocabulary::lookup_card_color;

/// Recognizes one color characteristic from Oracle vocabulary. This remains
/// independent from mana-symbol parsing: colors filter objects, not payments.
pub(in crate::oracle::canonical) fn card_color_from_oracle_word(raw: &str) -> Option<CardColor> {
    lookup_card_color(raw)
}

pub(in crate::oracle::canonical) const fn canonical_color_name(color: CardColor) -> &'static str {
    color.ir_name()
}
