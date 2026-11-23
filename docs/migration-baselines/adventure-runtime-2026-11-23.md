# Adventure runtime replay delta (2026-11-23)

Probe: `examples/league_support_probe.rs` on `.tmp/released-vs-main/{league,corpus}-input.jsonl`, built in a private target.
Baseline: main at 5093cec. After: PR #69 (adventure-runtime-fix).

| Measure | Baseline | After | Gained | Lost | IR changed |
|---|---:|---:|---:|---:|---:|
| league | 824 | 828 | 4 | 0 | 0 |
| corpus | 15079 | 15155 | 76 | 0 | 0 |

Adventure-layout cards in the corpus probe: 167; supported after: 76.

## State transitions (first blocking state, Adventure layout only)

| Before | After | Cards |
|---|---|---:|
| `card-structure.layout-runtime` | `supported` | 76 |
| `effect.start` | `effect.start` | 9 |
| `card-structure.layout-runtime` | `card-structure.omen-runtime` | 9 |
| `token.named.catalog-identity` | `token.named.catalog-identity` | 5 |
| `numeric.atom.start` | `numeric.atom.start` | 5 |
| `reminder.named-token.subject` | `reminder.named-token.subject` | 4 |
| `selector.predicate.start` | `selector.predicate.start` | 4 |
| `effect.damage.action` | `effect.damage.action` | 3 |
| `sequence.connector-or-end` | `sequence.connector-or-end` | 3 |
| `engine:` | `engine:` | 3 |
| `collection.card-noun` | `collection.card-noun` | 3 |
| `reference.source.unresolved-name` | `reference.source.unresolved-name` | 3 |
| `condition.sentence.atom-kind` | `condition.sentence.atom-kind` | 3 |
| `static.continuous.predicate` | `static.continuous.predicate` | 2 |
| `selector.controller-qualifier.relation-or-negation` | `selector.controller-qualifier.relation-or-negation` | 2 |
| `target-list.slot.start` | `target-list.slot.start` | 2 |
| `static.spell-cost-reduction.cost` | `static.spell-cost-reduction.cost` | 2 |
| `effect.unsupported-verb` | `effect.unsupported-verb` | 2 |
| `lower.modify-stats.another-target-relation` | `lower.modify-stats.another-target-relation` | 2 |
| `ability-frame.conditional.body` | `ability-frame.conditional.body` | 2 |
| `selector.search-card.card-noun` | `selector.search-card.card-noun` | 1 |
| `effect.damage.destination` | `effect.damage.destination` | 1 |
| `binding.chosen-creature-type.source` | `binding.chosen-creature-type.source` | 1 |
| `trigger.source-program.tapped-transition` | `trigger.source-program.tapped-transition` | 1 |
| `replacement.entry-counters.terminator` | `replacement.entry-counters.terminator` | 1 |
| `effect.combat-restriction.marker` | `effect.combat-restriction.marker` | 1 |
| `keyword.protection.quality` | `keyword.protection.quality` | 1 |
| `activation.discard-card.quantity` | `activation.discard-card.quantity` | 1 |
| `static.collection-keyword.predicate` | `static.collection-keyword.predicate` | 1 |
| `permanent.operand.complete` | `permanent.operand.complete` | 1 |
| `mana.policy.use` | `mana.policy.use` | 1 |
| `target-list.complete` | `target-list.complete` | 1 |
| `static.continuous.connector` | `static.continuous.connector` | 1 |
| `lower.card-action.bindings` | `lower.card-action.bindings` | 1 |
| `static.combat-restriction.complete` | `static.combat-restriction.complete` | 1 |
| `ability-frame.keyword.expansion` | `ability-frame.keyword.expansion` | 1 |
| `condition.static.source-state.property` | `condition.static.source-state.property` | 1 |
| `trigger.envelope.separator` | `trigger.envelope.separator` | 1 |
| `keyword-list.item` | `keyword-list.item` | 1 |
| `keyword.ward.cost` | `keyword.ward.cost` | 1 |
| `ability-frame.static.subject` | `ability-frame.static.subject` | 1 |
| `effect.double.quantity` | `effect.double.quantity` | 1 |
| `static.continuous.entry-modification` | `static.continuous.entry-modification` | 1 |

## Cards whose status changed

| Card | Before | After | League |
|---|---|---|---|
| Animating Faerie // Bring to Life | `card-structure.layout-runtime` | `supported` |  |
| Aquatic Alchemist // Bubble Up | `card-structure.layout-runtime` | `supported` |  |
| Ardenvale Tactician // Dizzying Swoop | `card-structure.layout-runtime` | `supported` |  |
| Beanstalk Wurm // Plant Beans | `card-structure.layout-runtime` | `supported` |  |
| Bellowing Bruiser // Beat a Path | `card-structure.layout-runtime` | `supported` |  |
| Beluna's Gatekeeper // Entry Denied | `card-structure.layout-runtime` | `supported` |  |
| Beorn, Reluctant Host // Till and Tend | `card-structure.layout-runtime` | `supported` |  |
| Bilbo Baggins, Burglar // Take a Glance | `card-structure.layout-runtime` | `supported` |  |
| Blessed Hippogriff // Tyr's Blessing | `card-structure.layout-runtime` | `supported` |  |
| Bofur, Reliable Guardian // Concerted Care | `card-structure.layout-runtime` | `supported` |  |
| Brazen Borrower // Petty Theft | `card-structure.layout-runtime` | `supported` | yes |
| Brightcap Badger // Fungus Frolic | `card-structure.layout-runtime` | `supported` |  |
| Colossal Badger // Dig Deep | `card-structure.layout-runtime` | `supported` |  |
| Crystal Dragon // Rob the Hoard | `card-structure.layout-runtime` | `supported` |  |
| Dirgur Island Dragon // Skimming Strike | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Disruptive Stormbrood // Petty Revenge | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Dread Linnorm // Scale Deflection | `card-structure.layout-runtime` | `supported` |  |
| Embereth Shieldbreaker // Battle Display | `card-structure.layout-runtime` | `supported` |  |
| Ettercap // Web Shot | `card-structure.layout-runtime` | `supported` |  |
| Faerie Guidemother // Gift of the Fae | `card-structure.layout-runtime` | `supported` |  |
| Fang Dragon // Forktail Sweep | `card-structure.layout-runtime` | `supported` |  |
| Fell Horseman // Deathly Ride | `card-structure.layout-runtime` | `supported` |  |
| Feral Deathgorger // Dusk Sight | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Flaxen Intruder // Welcome Home | `card-structure.layout-runtime` | `supported` |  |
| Foulmire Knight // Profane Insight | `card-structure.layout-runtime` | `supported` |  |
| Frolicking Familiar // Blow Off Steam | `card-structure.layout-runtime` | `supported` |  |
| Galvanic Giant // Storm Reading | `card-structure.layout-runtime` | `supported` |  |
| Garenbrig Carver // Shield's Might | `card-structure.layout-runtime` | `supported` |  |
| Ghost Lantern // Bind Spirit | `card-structure.layout-runtime` | `supported` |  |
| Giant Killer // Chop Down | `card-structure.layout-runtime` | `supported` |  |
| Gingerbread Hunter // Puny Snack | `card-structure.layout-runtime` | `supported` |  |
| Glóin the Mighty // Easy Pickings | `card-structure.layout-runtime` | `supported` |  |
| Gollum, Silent Slinker // Meager Meal | `card-structure.layout-runtime` | `supported` |  |
| Gray Slaad // Entropic Decay | `card-structure.layout-runtime` | `supported` |  |
| Great Ugly-Looking Goblin // Clap! Snap! | `card-structure.layout-runtime` | `supported` |  |
| Hezrou // Demonic Stench | `card-structure.layout-runtime` | `supported` |  |
| Horn of Valhalla // Ysgard's Call | `card-structure.layout-runtime` | `supported` |  |
| Hypnotic Sprite // Mesmeric Glare | `card-structure.layout-runtime` | `supported` |  |
| Imodane's Recruiter // Train Troops | `card-structure.layout-runtime` | `supported` |  |
| Intrepid Trufflesnout // Go Hog Wild | `card-structure.layout-runtime` | `supported` |  |
| Ishgard, the Holy See // Faith & Grief | `card-structure.layout-runtime` | `supported` |  |
| Lindblum, Industrial Regency // Mage Siege | `card-structure.layout-runtime` | `supported` |  |
| Lonesome Unicorn // Rider in Need | `card-structure.layout-runtime` | `supported` |  |
| Merchant of the Vale // Haggle | `card-structure.layout-runtime` | `supported` |  |
| Merfolk Secretkeeper // Venture Deeper | `card-structure.layout-runtime` | `supported` |  |
| Midgar, City of Mako // Reactor Raid | `card-structure.layout-runtime` | `supported` | yes |
| Minecart Daredevil // Ride the Rails | `card-structure.layout-runtime` | `supported` |  |
| Most Decrepit Old Bird // Speak Secrets | `card-structure.layout-runtime` | `supported` |  |
| Murderous Rider // Swift End | `card-structure.layout-runtime` | `supported` | yes |
| Oakhame Ranger // Bring Back | `card-structure.layout-runtime` | `supported` |  |
| Obyra's Attendants // Desperate Parry | `card-structure.layout-runtime` | `supported` |  |
| Order of Midnight // Alter Fate | `card-structure.layout-runtime` | `supported` |  |
| Queen of Ice // Rage of Winter | `card-structure.layout-runtime` | `supported` |  |
| Ratcatcher Trainee // Pest Problem | `card-structure.layout-runtime` | `supported` |  |
| Realm-Cloaked Giant // Cast Off | `card-structure.layout-runtime` | `supported` |  |
| Riling Dawnbreaker // Signaling Roar | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Rimrock Knight // Boulder Rush | `card-structure.layout-runtime` | `supported` |  |
| Rosethorn Acolyte // Seasonal Ritual | `card-structure.layout-runtime` | `supported` |  |
| Runescale Stormbrood // Chilling Screech | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Sagu Wildling // Roost Seek | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Sapphire Dragon // Psionic Pulse | `card-structure.layout-runtime` | `supported` |  |
| Scalding Viper // Steam Clean | `card-structure.layout-runtime` | `supported` |  |
| Shepherd of the Flock // Usher to Safety | `card-structure.layout-runtime` | `supported` |  |
| Silverflame Squire // On Alert | `card-structure.layout-runtime` | `supported` |  |
| Smaug, the Great Calamity // Spew Flame | `card-structure.layout-runtime` | `supported` |  |
| Smitten Swordmaster // Curry Favor | `card-structure.layout-runtime` | `supported` |  |
| Spellscorn Coven // Take It Back | `card-structure.layout-runtime` | `supported` |  |
| Stonehide Ancient // Warning Tremor | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Stormkeld Vanguard // Bear Down | `card-structure.layout-runtime` | `supported` | yes |
| Sword Coast Serpent // Capsizing Wave | `card-structure.layout-runtime` | `supported` |  |
| Tempest Hart // Scan the Clouds | `card-structure.layout-runtime` | `supported` |  |
| The Arkenstone // Seek the Heart | `card-structure.layout-runtime` | `supported` |  |
| Thranduil, Sindarin Liege // Silvan Rally | `card-structure.layout-runtime` | `supported` |  |
| Threadbind Clique // Rip the Seams | `card-structure.layout-runtime` | `supported` |  |
| Topaz Dragon // Entropic Cloud | `card-structure.layout-runtime` | `supported` |  |
| Tuinvale Treefolk // Oaken Boon | `card-structure.layout-runtime` | `supported` |  |
| Twining Twins // Swift Spiral | `card-structure.layout-runtime` | `supported` |  |
| Twinmaw Stormbrood // Charring Bite | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Two-Headed Hunter // Twice the Rage | `card-structure.layout-runtime` | `supported` |  |
| Velvetwing Butterflies // Gaze in Wonder | `card-structure.layout-runtime` | `supported` |  |
| Virtue of Persistence // Locthwain Scorn | `card-structure.layout-runtime` | `supported` |  |
| Whirlwing Stormbrood // Dynamic Soar | `card-structure.layout-runtime` | `card-structure.omen-runtime` |  |
| Woodland Acolyte // Mend the Wilds | `card-structure.layout-runtime` | `supported` |  |
| Young Blue Dragon // Sand Augury | `card-structure.layout-runtime` | `supported` |  |
| Zanarkand, Ancient Metropolis // Lasting Fayth | `card-structure.layout-runtime` | `supported` |  |
