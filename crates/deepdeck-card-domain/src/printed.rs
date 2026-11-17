use crate::registry::query_at;
use crate::{NominalError, SubtypeFamilies, SubtypeFamily, SubtypeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrintedSubtype {
    pub id: SubtypeId,
    pub family: SubtypeFamily,
}

/// Constructed only after the complete suffix has been validated. A false
/// contains result on this type is a proved absence, never a partial parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintedSubtypeSet {
    members: Vec<PrintedSubtype>,
}

impl PrintedSubtypeSet {
    pub fn iter(&self) -> std::slice::Iter<'_, PrintedSubtype> {
        self.members.iter()
    }

    pub fn contains(&self, id: SubtypeId) -> bool {
        self.members.iter().any(|member| member.id == id)
    }

    pub fn creature_types(&self) -> impl Iterator<Item = SubtypeId> + '_ {
        self.members
            .iter()
            .filter_map(|member| (member.family == SubtypeFamily::Creature).then_some(member.id))
    }
}

fn prefix_families(prefix: &str) -> Result<SubtypeFamilies, NominalError> {
    let mut families = SubtypeFamilies::default();
    let mut card_types = 0;
    for word in prefix.split_whitespace() {
        let family = match word.to_ascii_lowercase().as_str() {
            // Token is a carrier label in local token definitions, not a card
            // type, inferred isToken flag, or extra nominal family.
            "basic" | "legendary" | "ongoing" | "snow" | "world" | "token" => continue,
            "artifact" => Some(SubtypeFamily::Artifact),
            "battle" => Some(SubtypeFamily::Battle),
            "creature" | "kindred" => Some(SubtypeFamily::Creature),
            "enchantment" => Some(SubtypeFamily::Enchantment),
            "land" => Some(SubtypeFamily::Land),
            "planeswalker" => Some(SubtypeFamily::Planeswalker),
            "instant" | "sorcery" => Some(SubtypeFamily::Spell),
            "plane" => return Err(NominalError::UnsupportedPrintedFamily(SubtypeFamily::Plane)),
            "dungeon" => {
                return Err(NominalError::UnsupportedPrintedFamily(
                    SubtypeFamily::Dungeon,
                ));
            }
            // CR205.3r: these supported headers have no subtype family.
            "conspiracy" | "phenomenon" | "scheme" | "vanguard" => None,
            _ => return Err(NominalError::UnknownTypeWord(word.to_owned())),
        };
        card_types += 1;
        if let Some(family) = family {
            families = families.union(SubtypeFamilies::single(family));
        }
    }
    if card_types == 0 {
        return Err(NominalError::MissingCardType);
    }
    Ok(families)
}

/// CR205.3b/c nominal segmentation, separate from ability grammar. It retains
/// punctuation inside words, consumes the longest registered complete nominal
/// (Time Lord), and correlates each nominal with an actual prefix type family.
/// No card type is inserted; a Kindred spell may carry creature subtypes.
pub fn parse_printed_type_line(type_line: &str) -> Result<PrintedSubtypeSet, NominalError> {
    if type_line.trim().is_empty() {
        return Err(NominalError::EmptyTypeLine);
    }
    if type_line.contains("//") || type_line.contains('\n') || type_line.contains('\r') {
        return Err(NominalError::MalformedTypeLine(
            "composite/multiple faces must be planned separately".to_owned(),
        ));
    }
    let split = type_line
        .split_once('—')
        .or_else(|| type_line.split_once(" - "))
        .or_else(|| type_line.split_once(" – "));
    let (prefix, suffix) = split.map_or((type_line, None), |(a, b)| (a, Some(b)));
    let type_families = prefix_families(prefix)?;
    let Some(suffix) = suffix else {
        return Ok(PrintedSubtypeSet {
            members: Vec::new(),
        });
    };
    if suffix.trim().is_empty() {
        return Err(NominalError::MalformedTypeLine(
            "empty subtype suffix".to_owned(),
        ));
    }
    let words = suffix.split_whitespace().collect::<Vec<_>>();
    let mut members = Vec::new();
    let mut position = 0;
    while position < words.len() {
        // Only registered canonical/orthographic surfaces participate, never
        // plural grammar aliases or arbitrary capitalized/partial words.
        let pair = words
            .get(position + 1)
            .map(|second| format!("{} {second}", words[position]));
        let mut found = None;
        if let Some(pair) = pair.as_deref() {
            if let Some(value) = query_at(pair)? {
                found = Some((value, 2));
            }
        }
        if found.is_none() {
            if let Some(value) = query_at(words[position])? {
                found = Some((value, 1));
            }
        }
        let Some((id, width)) = found else {
            if let Some(pair) = pair.as_deref() {
                match crate::resolve_surface(pair, crate::AcceptedForms::SingularOrPlural) {
                    Ok(_) => {
                        return Err(NominalError::PrintedPluralNotAllowed {
                            surface: pair.to_owned(),
                            token_index: position,
                        });
                    }
                    Err(NominalError::UnknownSurface(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            return Err(
                match crate::resolve_surface(
                    words[position],
                    crate::AcceptedForms::SingularOrPlural,
                ) {
                    Ok(_) => NominalError::PrintedPluralNotAllowed {
                        surface: words[position].to_owned(),
                        token_index: position,
                    },
                    Err(NominalError::UnknownSurface(_)) => NominalError::UnknownPrintedSubtype {
                        surface: words[position].to_owned(),
                        token_index: position,
                    },
                    Err(error) => error,
                },
            );
        };
        let applicable = id.families().intersection(type_families);
        if applicable.is_empty() {
            return Err(NominalError::PrintedFamilyMismatch {
                surface: words[position..position + width].join(" "),
                nominal_families: id.families(),
                type_families,
            });
        }
        let family = applicable
            .only()
            .ok_or_else(|| NominalError::AmbiguousPrintedSubtype {
                surface: words[position..position + width].join(" "),
                families: applicable,
            })?;
        if members
            .iter()
            .any(|member: &PrintedSubtype| member.id == id)
        {
            return Err(NominalError::DuplicatePrintedSubtype(
                id.canonical_name().to_owned(),
            ));
        }
        members.push(PrintedSubtype { id, family });
        position += width;
    }
    Ok(PrintedSubtypeSet { members })
}
