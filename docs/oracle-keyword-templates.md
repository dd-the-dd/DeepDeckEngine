# Mots-clés compilés depuis leur définition (C6a)

Ce document décrit le mécanisme qui compile un mot-clé (CR 702) en
instanciant sa définition des règles et en la passant par l'analyseur
ordinaire. Il complète `oracle-compositional-compiler.md` (workflow des
nouveaux mots-clés : définition faisant autorité, expansion typée, politique
d'événement, rejeu) et `oracle-clause-layer.md`.

## Principe

Un mot-clé « macro » (Modular, Fabricate, Battle cry, Outlast…) est défini par
la CR comme un texte de règles ordinaire : « *Modular N* means *This permanent
enters with N +1/+1 counters on it* and *When this permanent is put into a
graveyard from the battlefield, …* ». Au lieu d'écrire à la main le programme
JSON de chaque mot-clé, une ligne de données porte cette définition avec des
emplacements de paramètres. À la compilation :

1. la ligne de mot-clé est reconnue (lexème, paramètre imprimé lu par les
   enfants partagés de nombre et de coût de mana, rappel facultatif validé) ;
2. la définition est instanciée avec le paramètre imprimé ;
3. chaque capacité instanciée est compilée par le **même** analyseur par
   capacité que le texte imprimé, dans le contexte de la face (nom, ligne de
   type, résolution des noms) ;
4. le résultat est une règle `keywordExpansion` qui porte la provenance et les
   règles compilées ; l'adaptateur de cartes jouables l'aplatit.

Le mot-clé hérite ainsi de toutes les capacités de grammaire et d'exécution.
Un mot-clé dont la définition se compose de concepts déjà pris en charge ne
coûte qu'une ligne de données ; une correction de grammaire débloque
automatiquement les mots-clés dont la définition l'utilise.

## Données : `data/ability-catalog/keyword-templates-v1.json`

Schéma `deepdeck-keyword-templates/v1`, versionné avec le catalogue
(`catalogVersion`, `rulesEffectiveDate` identiques à l'index embarqué).

| Champ | Rôle |
|---|---|
| `canonicalId` | identité du catalogue (`ability-catalog-v1.json`, catégorie `keywordAbility`) |
| `templateVersion` | version de la ligne ; à incrémenter si la définition change |
| `lexeme` | nom imprimé, tokenisé par le tokenizer partagé (`For Mirrodin!`) |
| `parameter` | `none`, `count` (N), `manaCost` ([cost]), `countThenManaCost` (N—[cost], CR 702.77a) |
| `rulesClause`, `rulesSection` | clause CR en cache citée (ex. `modular.cr-702-43a`, `702.43a`) |
| `faceSubtype` | sous-type exigé quand la définition nomme la source par lui (`this Equipment`) |
| `abilities` | une capacité Oracle par capacité représentée par le mot-clé |
| `reminders` | rendus de rappel acceptés (sans parenthèses) |

Emplacements (définitions et rappels) :

| Emplacement | Rendu |
|---|---|
| `$N` | le nombre imprimé en chiffres (`+$N/+$N`, `loses $N life`) |
| `$Nw` | le nombre en toutes lettres (`two`), jusqu'à vingt |
| `$N{UN|PLUSIEURS}` | accord en nombre : `UN` si N = 1, sinon `PLUSIEURS` |
| `$cost` | le coût de mana tel qu'imprimé |
| `$Nrepeat[TEXTE]` | TEXTE écrit N fois (`{R}{R}` ; forme CR `N TEXTE`) |
| `$Ntimes[TEXTE]` | TEXTE, `twice TEXTE`, `three times TEXTE`… (multiple, rappel de Devour) |
| `$subtypes` | les sous-types de la face, dans l'ordre imprimé (rappels seulement) |
| `$subtypeAlternatives` | les sous-types joints par `and/or` (`Bird and/or Soldier`, rappels seulement) |

Un rappel peut reformuler la définition avec les sous-types de la carte
(Embalm : `a white Zombie Human Warrior`, Amplify : `each Beast card`) : ces
emplacements lisent la ligne de type de la face et sont refusés dans une
définition, qui cite la forme symbolique de la CR (`render_rules_form`
échoue).

L'accord est explicite dans les données parce que la grammaire vérifie le
nombre (`put a +1/+1 counter`, `put two +1/+1 counters`) ; aucune flexion
n'est déduite. Un emplacement inconnu ou sans argument est une erreur, jamais
un blanc.

### Preuve de provenance

`keyword_template_tests.rs` vérifie pour chaque ligne :

- l'identité existe dans le catalogue en cache, catégorie `keywordAbility` ;
- la clause citée existe et sa section correspond ;
- chaque capacité, rendue sous sa forme symbolique de la CR (`N`, `[cost]`,
  branche plurielle), est une **citation exacte** de la clause en cache
  (apostrophes et guillemets typographiques normalisés, casse ignorée,
  point final de citation ignoré) ;
- aucune définition n'est elle-même une ligne de mot-clé (expansion à un seul
  niveau, sans récursion) ;
- chaque définition et chaque rappel s'instancient pour chaque forme de
  paramètre.

## Reconnaissance (`automata/keyword_template.rs`)

```text
Ligne     := LEXÈME Paramètre [ "(" Rappel ")" ]
Paramètre := ε | N | COÛT-DE-MANA | N "—" COÛT-DE-MANA
```

- Revendication structurelle : le lexème en tête **immédiatement** suivi du
  début de son paramètre (fin de ligne ou `(` pour `none`, un nombre, un
  symbole de mana). `Battle cry creatures get…`, `Unleash Shell deals…`,
  `Scavenge the Dead —` ou une liste `Flying, battle cry` ne sont pas
  revendiqués.
- Après revendication, tout échec est explicite (`keyword.template.*`) : compte
  non littéral (`Reinforce X—…`), coût de mana non fixe, rappel étranger
  (`keyword.template.reminder.body`), texte après le rappel, sous-type de face
  manquant (`keyword.template.face-subtype`).
- Le rappel n'a pas de sens de règle (CR 207.2) mais n'est jamais ignoré : il
  doit égaler, token par token, un rendu déclaré instancié avec le paramètre.

## Expansion (`abilities/keyword_template_expansion.rs`, `pipeline.rs`)

Le pipeline prépare chaque capacité (`prepare_ability` : tokens, jetons
nommés, noms de marqueurs, résolution des noms), puis appelle
`expand_keyword_template_line` avec une fermeture qui compile un texte par ce
même chemin, dans la même face. Une partie qui échoue fait échouer le mot-clé :

- échec principal `keyword-template` / `keyword.template.expansion`, couvrant
  la ligne imprimée, nommant le mot-clé, la section CR, la partie instanciée et
  l'état où l'analyseur s'est arrêté ;
- l'échec de la partie est conservé comme preuve candidate
  (`oracle_candidate_machine_failure`).

IR avant aplatissement :

```json
{"kind": "keywordExpansion", "source": {"kind": "self"},
 "keyword": "modular", "parameters": {"N": 2},
 "definition": {"rulesSection": "702.43a", "rulesClause": "modular.cr-702-43a",
                "templateVersion": 1, "catalogVersion": "…", "rulesEffectiveDate": "…"},
 "rules": [RÈGLE, RÈGLE]}
```

`keyword_expansion_rules` (exporté par `deepdeck_oracle::oracle`) rend les
règles jouables : chaque règle compilée, avec les attributs de placement de
l'expansion (`minimumClassLevel`, `roomDoorIndex`…) et un champ
`keywordProvenance` `{keyword, parameters, rulesSection, templateVersion,
part}`. Toute autre règle est rendue inchangée. Le moteur ne connaît pas
`keywordExpansion` : une expansion non aplatie reste non exécutable (échec
fermé). `playable_rules_for_face` (`src/model.rs`) est le seul point
d'aplatissement.

Chaque instance d'un mot-clé produit ses propres règles : « chaque instance
fonctionne séparément » (CR 702.43b, 702.45b, …) est satisfait sans code.

## Politique d'événement

Un mot-clé qui définit un événement nommé (Exploit : « exploite une
créature », CR 702.110b ; Renown : « devient renommé » ; Evolve : « évolue »)
n'est ajouté que lorsque cet événement est publié par le moteur. Une
définition compilée ne publie pas d'événement du seul fait du mot-clé.

La ligne déclare cette politique (`namedAction`) : le lemme de l'action et
l'action moteur qui l'accomplit (`performedBy: sacrifice`). L'expansion la
recopie dans `keywordProvenance.namedAction`. Quand une règle qui la porte a
fini de se résoudre, `keyword_actions.rs` publie un enregistrement du lemme
pour chaque action de ce type accomplie par son contrôleur pendant la
résolution (sacrifices du journal entre le début et la fin) : sujet = la
source (même si elle s'est sacrifiée elle-même), objet = le permanent
sacrifié. Un déclencheur le lit par un motif générique :

```json
{"kind": "eventPattern", "lemma": "exploit",
 "subject": {"object": {"kind": "self"}},
 "counterpart": {"where": {"kind": "cardTypeContains", "value": "Creature"}}}
```

L'analyseur lit `SOURCE exploits QUERY` par le lemme `exploit` du lexique
des verbes (cadre `ObjectSelection`) et le lecteur de contrepartie partagé.
Restent explicites : un autre sujet que la source (`a creature you control
exploits`), une contrepartie avec contrôleur ou caractéristique numérique.

## Migration des programmes natifs

Une ligne remplace un programme natif seulement si la définition compilée
donne une IR exécutable équivalente, prouvée par test.

| Mot-clé | Décision | Preuve / raison |
|---|---|---|
| Bushido (702.45a) | migré | IR identique à l'ancien programme natif (`keyword_template_fsm.rs`), tests runtime de blocage existants |
| Living weapon (702.92a) | migré | IR identique ; `faceSubtype: Equipment` conserve le refus d'une face non-Équipement |
| Afterlife (702.135a) | migré, correction assumée | la CR dit « put into a graveyard from the battlefield » : `permanentLeftBattlefield` vers le cimetière au lieu de `permanentDied` (créature seulement). Même comportement pour une créature (preuve runtime : deux Esprits volants à la destruction) ; correct aussi pour un permanent non-créature |
| Soulshift (702.46a) | migré, même correction | même événement que la définition CR |
| Toxic (702.164) | natif | pas de définition « means » : capacité statique dont la règle est intrinsèque (CR 702.164c, 120.3) |
| Annihilator (702.86a) | natif | la définition compile en un déclencheur différent du mot-clé intrinsèque `annihilator` du moteur ; équivalence non prouvée |
| Afflict (702.130a) | natif | `defending player` après `becomes blocked` non lié (`player-subject.defending-player`) |
| Riot (702.136a) | natif | choix d'entrée « If you don't, it gains haste » (`ability-frame.entry-replacement.modification`) |
| Echo (702.30a) | natif | condition « came under your control since the beginning of your last upkeep » (`condition.sentence.object.verb`) |
| Cumulative upkeep (702.24a) | natif | paiement « for each age counter » (`selector.predicate.start`) |
| Buyback, Evoke, Offspring | natifs | cadres de coût additionnel/alternatif à la permission (`ability-frame.permission.action`, `effect.start`) |
| Affinity (702.41a) | natif | IR identique pour « artifacts », mais « for each [text] » exige le singulier de la qualité imprimée ; le nœud typé natif l'évite |
| Cascade (`keyword_programs.rs`) | natif | programme moteur paramétré par la valeur de mana capturée au lancement, pas un texte Oracle |

## Lignes ajoutées

| Mot-clé | CR | Paramètre | Preuve runtime |
|---|---|---|---|
| Battle cry | 702.91a | — | `accorder_paladin_battle_cry_pumps_each_other_attacker_once` |
| Fabricate | 702.123a | N | `entry_keywords_create_and_attach_or_grow_from_their_definitions` |
| For Mirrodin! | 702.163a | — (`faceSubtype: Equipment`) | idem (jeton Rebel équipé) |
| Job select | 702.182a | — (`faceSubtype: Equipment`) | même IR que For Mirrodin! (jeton Hero) |
| Outlast | 702.107a | [cost] | `outlast_and_reinforce_activate_from_their_definitions_zones` |
| Reinforce | 702.77a | N—[cost] | idem (activation depuis la main seulement) |
| Frenzy | 702.68a | N | motif `becomeUnblocked` existant (famille d'attaque) |
| Exploit | 702.110a | — (`namedAction: exploit`) | `exploit_publishes_its_keyword_action_only_when_a_creature_is_sacrificed` |
| Vanishing | 702.63a | N | `vanishing_removes_a_time_counter_each_upkeep_and_sacrifices_at_the_last` |
| Bloodthirst | 702.54a | N | `bloodthirst_counters_depend_on_damage_dealt_to_an_opponent_this_turn` |
| Modular | 702.43a | N | `modular_moves_the_last_known_counters_of_its_dead_source` (dernière information connue, ci-dessous) |
| Crew | 702.122a | N | `crew_taps_creatures_with_enough_total_power_and_animates_the_vehicle`, `crew_rejects_a_declared_set_below_the_total_power` (coût par force totale, ci-dessous) |

## Coût de force totale (Crew, Saddle, coûts imprimés)

`Tap any number of [other] [untapped] QUERY with total power N or greater`
(CR 702.122a, 702.171a) est une action de coût ordinaire de la séquence de
coûts partagée (`cost.rs`, `recognize_total_power_tap_cost`) : la quantité
`any number of` n'est pas un compte mais une borne agrégée sur l'ensemble
choisi. La requête d'objets partagée est bornée par `with total power` et doit
nommer des permanents du payeur (`you control`) ; `or greater` et `or more`
sont la même borne inférieure ; `or less` échoue explicitement
(`cost.total-power-tap.comparison`). `other` exclut la source.

IR (projection d'activation, `activated_native.rs`) :

```json
{"kind": "tapPermanentsWithTotalPower", "decisionId": "activationCostTapPower0",
 "where": {"kind": "cardTypeContains", "value": "Creature"},
 "minimumPower": {"kind": "integer", "value": 3}, "excludeSource": true}
```

Moteur (`src/engine/creature_tap_cost.rs`, crochets d'une ligne dans
`mod.rs`) : la déclaration d'activation énumère les ensembles légaux (plus
petits d'abord, dans le budget partagé de déclarations) ; avant tout paiement
l'ensemble déclaré est revérifié (permanents dégagés du payeur, filtre
effectif, source exclue si `other` ou si un `{T}` la dégage, pas de doublon,
pas de source de mana du même paiement, somme des forces courantes ≥ N), puis
chaque permanent est engagé et ses déclencheurs « devient engagé » publiés.
Le mal d'invocation ne restreint pas ce coût (ce n'est pas `{T}`, CR 302.6).
Un coût de résolution ou `unless` n'a pas de schéma pour cette action : échec
explicite. Les coûts imprimés de même forme (`Tap any number of other
untapped creatures you control with total power 4 or greater: …`) composent
sans code (cas frère des fixtures `keyword-template-crew.json`).

Restent explicites : `crews Vehicles as though its power were N greater`
(lemme `crew` absent du lexique des verbes), `crews Vehicles using its
toughness`, `Whenever this Vehicle becomes crewed` (événement CR 702.122e non
publié), `Crew N. Activate only once each turn.`

## Retour de la carte source depuis le cimetière

L'enfant partagé de déplacement de la source (`source_zone_move.rs`) lit
`return this card from your graveyard to the battlefield [tapped]` (CR 110.2 :
sous le contrôle du joueur qui la met en jeu, le contrôleur de la capacité ;
CR 110.5 : engagée). L'IR est le `moveCards` existant (`from` cimetière,
`to.tapped`) ; l'origine cimetière fixe `activationZone: graveyard`. C'est la
première partie d'Unearth et une forme imprimée fréquente (Reassembling
Skeleton, Bloodghast, Drownyard Temple). `attached to …`, `with a finality
counter on it` et `tapped and attacking` restent non lus (échec de la séquence
englobante). Preuves : `graveyard-source-return.json`,
`a_graveyard_activation_returns_its_own_card_to_the_battlefield_tapped`,
`an_untapped_graveyard_return_enters_untapped`.

## Défauts trouvés par les preuves runtime

- `modifyPowerToughness` sur `eachPermanent` lisait son filtre sur la carte
  imprimée : `each other attacking creature` ne correspondait jamais. Le
  filtre est lu par `permanent_matches_effective_filter` (état et
  caractéristiques effectives, CR 608.2h), comme l'exil le faisait déjà. Sans
  changement d'IR : les cartes imprimées concernées étaient déjà « supportées »
  mais sans effet.
- `create … token, then attach this to it` référençait la liaison
  `createdTokens` sans que la création la déclare : l'attachement était sans
  effet (y compris pour l'ancien living weapon natif). Un `attach` dont
  l'objet ou le receveur est le jeton créé fait maintenant lier la création,
  comme les autres consommateurs (`created_tokens_are_next_antecedent`).
  Changement d'IR assumé : ajout de `"bind": "createdTokens"`.

## Dernière information connue des déclencheurs « regard en arrière »

Un déclencheur de la source elle-même publié après son départ du champ de
bataille (`dies`, `is put into a graveyard from the battlefield`, …) lit
l'information de dernier état de cette incarnation (CR 603.10a). La sortie
centrale `capture_pending_source_lki` enregistre déjà la capture de départ
authentifiée (force, endurance, traits de source de dégâts, objet avec ses
marqueurs) pour les capacités en attente ; elle la conserve désormais aussi
par incarnation (`SourceLkiContexts::departed`). `publish_triggered_ability`
la donne à un déclencheur dont la source n'est plus sur le champ de bataille
dans cette incarnation. Les lectures vérifiées de la source (marqueurs,
caractéristiques, source de dégâts) acceptent alors cette capture au lieu
d'échouer : le cas « exécutable à l'admission, refusé à l'exécution » relevé
avec Modular est fermé pour toute carte imprimée de même forme. Une sortie
hors des sorties centrales ne produit pas de capture et garde l'échec
explicite.

## Corrections de grammaire partagée

- **Condition d'entrée antéposée** (Bloodthirst, CR 702.54a) : le lecteur
  partagé `SOURCE enters with N COUNTER counters on it` acceptait déjà une
  condition `if CONDITION` après la modification ; il l'accepte aussi avant
  (`If CONDITION, SOURCE enters with …`), dans le même emplacement unique.
  Deux conditions (avant et après) échouent explicitement.
- **Historique de dégâts d'un adversaire** : `an opponent was dealt damage
  this turn` est un atome `EventHistory` du langage de condition unique
  (participe passé du lemme `deal` lu dans le lexique des verbes). IR :
  `countEventsThisTurn` sur `damageDealt`, relation de joueur `opponentsOf`,
  mesure `amount` (dégâts à un joueur adversaire ce tour, toute source,
  CR 120.3), évalué par le compteur d'historique générique du moteur.

## État des marqueurs (capacité partagée)

- **Possession** : un seul lecteur de quantité de marqueurs (`selector.rs`,
  `recognize_counter_qualifier`) sert les requêtes d'objets (`with three or
  more +1/+1 counters on it`, `with no counters on it`) et les conditions
  `has|had … counter(s) on it`. Le prédicat `HasCounter(CounterPresence)`
  porte le genre de marqueur et des bornes (`a` = au moins 1, `no` = 0, `N or
  more`, `fewer than N`, `N or fewer`). IR : l'atome `hasCounter` existant,
  avec `minimum`/`maximum` seulement quand ils diffèrent de « au moins un »
  (IR existante inchangée). Un compte exact (`three counters`) n'est pas
  revendiqué.
- **Condition intercalée à la publication** (CR 603.4) : le moteur évaluait
  `objectMatchesFilter` comme faux au moment de mettre un déclencheur sur la
  pile (bras manquant dans `condition_value_with_source`) ; tout déclencheur
  admis avec une telle condition ne se déclenchait jamais. Il est maintenant
  évalué par le même évaluateur qu'à la résolution, avec la source et ses
  décisions.
- **Retrait de marqueurs** (CR 122.8) : `remove N KIND counter(s) from
  OBJECT` est l'action de marqueurs de `put … on`, mêmes enfants (quantité,
  nom de marqueur, opérande de permanent), sens `Remove`
  (`CounterChangeDirection`). IR : `removeCounters` du moteur, qui ne retire
  que de l'objet de la capacité avec un compte littéral ; un autre objet
  échoue explicitement. Le coût `Remove … :` avait déjà son lecteur.
- **Dernier marqueur retiré** : `the last KIND counter is removed from
  SOURCE` (participe passif de `remove` du lexique) est l'événement
  `counterRemoved {last: true}` déjà exécuté pour Suspend.
- **Antécédent de la condition intercalée** : quand chaque atome du `if`
  intercalé porte sur la source (`if this permanent has a time counter on
  it`), le `it` du corps désigne la source si l'événement n'en a lié aucun.
- **Source partie** (CR 603.10a) : une condition sur la source d'un
  déclencheur « regard en arrière » lit la capture de départ (objet avec ses
  marqueurs), à la publication comme à la résolution, au lieu de la carte
  imprimée.

## Condition intercalée lue dans le cadre du déclencheur (Evolve)

La condition `if …` d'un déclencheur (CR 603.4) est lue avec
l'environnement de liaison C2 installé : son cadre est celui de l'événement
(`event_frame` : joueur, objet, carte, sort de l'événement), le même que
celui des effets du déclencheur. `that creature's power` y devient donc
`powerOf observedEventObject`, exactement comme dans un effet ; aucun
résolveur n'est dupliqué.

- Atome de phrase partagé `NumericRelation` : `EXPR is greater than |
  less than | equal to EXPR`, où les deux côtés sont des expressions
  numériques ordinaires (propriété d'un objet lié, propriété de la source,
  nombre). Les opérateurs stricts `<`/`>` rejoignent
  `NumericComparisonOperator` (`ir_operator` unique).
- `and/or` est le connecteur de disjonction de la liste contextuelle
  partagée (CR 702.100a : « greater power and/or … toughness »).
- Moteur : la condition est évaluée à la publication par l'évaluateur de
  résolution (`runtime_condition_value`) sur l'objet de pile en attente,
  avec la capture de départ de la source si elle a quitté le champ de
  bataille (CR 603.10a), au lieu de l'évaluateur statique qui ne connaît
  pas l'objet de l'événement.

Combinaisons sans code nouveau débloquées par le même cadre : la condition
d'un déclencheur de mort (`if it had a -1/-1 counter on it`, `if its
toughness was less than 1`) nomme maintenant l'objet de l'événement. Comme
il a quitté le champ de bataille, `objectMatchesFilter` le lit par
l'information de l'enregistrement de mort (CR 603.10a : marqueurs, état de
combat, force/endurance capturés), jamais par la carte dans sa nouvelle zone
(`object_condition.rs`, `LastKnown`). Preuve :
`death_trigger_guards_read_the_dead_creature_through_the_trigger_frame`.

Preuves : `evolve_guard_reads_the_event_object_through_the_trigger_frame`
(carte, condition sœur jamais vue `toughness … less than … power`, quasi
échec `bigger than`) et
`evolve_checks_the_entering_creature_against_its_source_when_it_triggers`
(puissance plus grande, égalité sans publication, endurance seule plus
grande, créature adverse ignorée).

## Comparaison stricte à une caractéristique de la source (Mentor)

La comparaison numérique du sélecteur (`with power …`) accepte les
opérateurs stricts `less than` / `greater than` (le même
`NumericComparisonOperator` que les conditions de phrase) et, comme
opérande droit, une caractéristique de la source (`this creature's power`,
`NumericExpression::ObjectProperty { SourceSelf }`).

- Moteur (`source_bound_operands.rs`) : un domaine de candidats de cible
  lie l'opérande de la source à sa valeur courante (sinon sa dernière
  information connue) quand les cibles sont choisies (déclencheur et
  capacité activée, CR 601.2c) et revérifiées à la résolution (CR 608.2b) ;
  chaque comparateur de candidats compare donc toujours à un entier.
- Ailleurs (statique, comptage), rien ne lie cette valeur : le contrôle de
  support refuse l'opérande (`contains_source_operand`) au lieu de comparer
  à une valeur absente ; la carte reste non exécutable, explicitement.

Preuves : `mentor_compares_the_candidate_with_a_source_characteristic`
(carte, sœur activée `toughness greater than this creature's toughness`,
quasi échec sans opérande) et
`mentor_targets_only_attackers_with_less_power_than_its_source` (seule la
créature attaquante de puissance inférieure est ciblable ; le filtre
statique équivalent n'est pas exécutable).

## Désignations (registre partagé : Renown, Unleash)

`automata/designation.rs` est le registre unique des désignations de
permanent (`monstrous` CR 701.37b, `renowned` CR 702.112b, `suspected`
CR 701.60b). Chaque consommateur le lit, aucun ne liste les mots :

- prédicat d'objet `ObjectPredicate::Designation` (`target renowned
  creature`, `if it isn't renowned`, `if it was renowned` après une mort) →
  `{"kind":"hasDesignation","designation":…}` ;
- condition d'état de la source (`as long as this creature is monstrous`)
  → `sourceFlagSet` ;
- action `OBJECT becomes DESIGNATION` (`it becomes renowned`) →
  `setPermanentFlag`, la moitié désignation de monstruosité (le programme de
  monstruosité lit maintenant le registre). `suspected` n'est donné que par
  l'action suspect, qui accorde aussi ses règles.
- Moteur : `hasDesignation` lit le drapeau du permanent ; l'enregistrement
  de mort capture les drapeaux (`triggeringPermanentFlags`) pour la vue
  « dernière information connue ».

Unleash (CR 702.98a) compose deux capacités partagées :

- `You may have SOURCE enter with …` est le remplacement d'entrée avec
  marqueurs déjà existant, appliqué au choix du contrôleur (décision de
  remplacement générique `chooseWhether`, condition `decisionWas`) ;
  `an additional` est le même placement (les marqueurs s'ajoutent).
- `as long as it has a +1/+1 counter on it` : condition statique de
  possession de marqueurs par la source, lue par le qualificatif de
  marqueurs partagé (`objectMatchesFilter` + `hasCounter`).

Preuves : `designations_come_from_one_registry_for_predicates_conditions_and_actions`,
`unleash_is_an_optional_entry_replacement_and_a_counter_condition` (carte,
sœurs sans code, quasi-échecs) et les preuves runtime
`designations_are_given_once_and_read_by_conditions_selectors_and_last_known_deaths`,
`unleash_lets_its_controller_choose_the_counter_that_stops_it_blocking` ;
la condition de possession compose sans code avec un effet continu sur
d'autres créatures et une restriction d'activation
(`source_counter_possession_gates_statics_and_activations_without_new_code` :
Beastmaster Ascension, Tablet of Compleation).

## Résultat d'une action et compte d'adversaires (Extort, Melee)

Extort (CR 702.101a) : `the [total] [amount of] life lost this way` est une
quantité produite par une action antérieure du même programme (CR 608.2c),
comme `OBJECTS destroyed this way` pour les objets. L'enfant numérique
(`action_result.rs`) note l'action qui la lit (l'index de l'environnement de
liaison) ; la descente du programme (`install_action_result_quantities`)
lie l'action antérieure la plus proche dont la descente est une seule
action `loseLife` de premier niveau (`bind`) et réécrit la lecture en
`boundValue`. Sans producteur, l'échec est explicite
(`lower.action-result.quantity`). Moteur : `loseLife` avec `bind` enregistre
la vie réellement perdue (la perte prévenue n'est pas perdue).

Melee (CR 702.121a) : `for each opponent you attacked [with a creature]
this combat` est un opérande compté (`OpponentCount`) →
`countOpponentsAttackedThisCombat` ; le moteur compte les adversaires
attaqués par les attaquants déclarés du joueur actif pendant ce combat
(attaquer un planeswalker ou une bataille n'est pas attaquer le joueur).
Le `for each` final multiplie désormais aussi le delta signé de
`gets +N/+M` (chaque composante ; zéro reste zéro ; un négatif est
`-1 × count`) : capacité générale, pas propre à Melee.

Preuves : `melee_scales_the_shared_stat_delta_by_an_opponent_count`
(carte, sœurs `-1/-0 … for each Swamp you control` et `draw a card for each
opponent you attacked this combat`, quasi-échec `for each opponent you
control`), `melee_counts_the_opponents_attacked_this_combat` (un, deux
adversaires, planeswalker seul) et
`the_life_lost_this_way_is_the_total_the_earlier_loss_recorded` (2 et
3 joueurs).

## Résultat d'un retrait : `If you can't` et déplacer des marqueurs (Fading, Graft)

`removeCounters` enregistre déjà, par `bind`, l'objet dont il a retiré les
marqueurs (vide s'il n'a pas pu). Deux capacités partagées lisent ce
résultat :

- `If you can't` (CR 609.3) est un atome de condition de phrase
  (`PreviousActionImpossible`). La descente du programme lie l'instruction
  juste avant la condition, qui doit être une seule action de premier
  niveau enregistrant son résultat (`removeCounters`), et produit
  `not(bindingNotEmpty)`. Sans telle action, échec explicite
  (`lower.sentence-condition.previous-action.producer`).
- `move COUNT KIND counter(s) from OBJECT onto OBJECT` (CR 122.5) : le
  verbe `move` du lexique lit exactement les enfants de `remove … from`,
  puis la destination de `put … on` ; il se compose en `removeCounters`
  (avec `bind`) puis `putCounters` conditionné par `bindingNotEmpty` : un
  marqueur non retiré n'est pas posé.
- `the permanent` (CR 702.32a) : une description définie `the NOUN` (domaine
  d'objet ou type de carte) désigne l'antécédent qu'un pronom désignerait,
  par le même résolveur (`recognize_source_reference_with_antecedent`) ;
  sans antécédent, échec explicite. **Changement minimal dans
  `reference.rs` (fichier C2), signalé.**

Preuves : `removal_results_drive_if_you_cant_and_counter_moves` (Fading,
Graft, sœur `remove a charge counter … If you can't, you lose 3 life`,
quasi-échecs `draw a card. If you can't` et `Sacrifice the creature.`),
`fading_sacrifices_when_it_cannot_remove_a_fade_counter`,
`graft_moves_a_counter_only_when_it_could_be_removed`.

## Mots-clés en attente (raison mesurée)

Ces lignes sont dans le catalogue : la ligne de mot-clé est lue et sa
définition compilée, et l'échec nomme la partie et l'état de la grammaire
partagée qui manque (`keyword.template.expansion`, preuve candidate). Une
correction de cette grammaire débloque le mot-clé sans code de plus.
Lignes du corpus (toutes cartes) par état d'arrêt de la définition :

| Mot-clé | Lignes | Arrêt de la définition |
|---|---|---|
| Training | 11 | `event.source.verb` (`this creature and at least one other … attack`) |
| Increment | 10 | `condition.sentence.atom-kind` (mana dépensée pour le sort) |
| Dethrone | 9 | `trigger.envelope.separator` (`attacks the player with the most life`) |
| Rampage | 9 | `sequence.for-each.action` (`for each creature blocking it beyond the first`) |
| Ingest | 9 | `effect.start` (`that player exiles the top card`) |
| Provoke | 8 | `choose.verb` |
| Decayed | 1 | `lower.delayed.object-references` (`sacrifice it at end of combat`) |
| Unearth | 57 | `effect.move-source.return.destination` résolu ; reste l'antécédent `It gains haste` après un retour de la source et `If it would leave the battlefield, exile it instead…` |
| Backup | 26 | `condition.sentence.atom-kind` (« the non-backup abilities printed below » : référence au texte imprimé) |
| Encore | 26 | `sequence.for-each-player.body` (copie par adversaire qui attaque ce joueur) |
| Saddle | 34 | effet `becomes saddled until end of turn` (désignation à durée) ; le coût est celui de Crew |
| Devour | 19 | `ability-frame.entry-replacement.modification` (`As this object enters, you may sacrifice …`) |
| Firebending | 18 | `effect.start` (`Until end of combat, you don't lose this mana…`) |
| Embalm | 15 | `reference.source.antecedent-kind` (copie `except it's white, it has no mana cost, …`) |
| Scavenge | 14 | `collection.card-noun` (`a number of … equal to the power of the card you exiled`) |
| Squad | 14 | `ability-frame.additional-cost.action` (`pay [cost] any number of times`) |
| Transmute | 14 | `selector.search-card.basic-land-type` (`with the same mana value as the discarded card`) |
| Enlist | 15 | `ability-frame.static.subject` (`As this creature attacks, you may tap …`) |
| Ravenous | 12 | `binding.x-value.source` (X du sort d'un permanent, CR 107.3m) |
| Eternalize | 11 | `effect.start` (copie `except it's black, it's 4/4, …`) |
| Amplify | 9 | `ability-frame.entry-replacement.modification` (révélation à l'entrée) |

Cinq lignes échouent sur un rappel de libellé ancien ou rare (`one fade
counter`, `enters the battlefield`, enchantement/terrain) : échec explicite
`keyword.template.reminder.body`.

Les rappels de ces lignes sont déjà validés (texte imprimé du corpus :
Unearth 49/51, Encore 25/25, Devour 19/19, Backup 23/24, Saddle 24/24,
Embalm 14/14, Eternalize 10/11, Transmute 13/14, Firebending 16/16) : seule la
définition reste à compiler, et chaque correction de la grammaire partagée
nommée ci-dessus débloque le mot-clé sans code de plus.

Non ajoutés : Station (une grammaire historique produit encore
`stationCreaturePower` en mode complet, et le rappel `It's an artifact
creature at N+` décrit la bande P/T de la carte, pas le mot-clé), Myriad
(programme moteur historique `resolveMyriad`), Champion (paramètre « an
[object] »), Double team (`conjure`).

Hors périmètre (mécanismes moteur nouveaux), cartes bloquées seulement par
eux : Madness 39, Partner 32 (règle de construction), Start your engines! 29,
Power-up 28, Visit 25, Exhaust 23, Spree 21, Ninjutsu 14, Sneak 14, Escape 12,
Banding 10, Blitz 9, Phasing 7.
