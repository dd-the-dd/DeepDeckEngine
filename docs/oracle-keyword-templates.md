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
| Modular | 702.43a | N | `modular_moves_the_last_known_counters_of_its_dead_source` (dernière information connue, ci-dessous) |

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

## Mots-clés en attente (raison mesurée)

| Mot-clé | Cartes corpus bloquées seulement par lui | Blocage |
|---|---|---|
| Backup | 18 | « gains the non-backup abilities printed below » : référence aux capacités imprimées |
| Bloodthirst | 15 | remplacement d'entrée conditionnel (`ability-frame.conditional.body`) |
| Unleash | 14 | entrée facultative avec marqueur, restriction conditionnelle |
| Renown | 13 | désignation « renowned » |
| Evolve | 12 | comparaison de caractéristiques de l'objet d'événement (`reference.source.start`) |
| Devour | 11 | « sacrifice any number … for each creature sacrificed this way » à l'entrée |
| Mentor, Extort | 11, 11 | comparaison de force à la cible ; « life lost this way » |
| Fading, Vanishing | 10, 9 | « If you can't, sacrifice the permanent » ; « when the last time counter is removed » |
| Graft | 10 | « move a +1/+1 counter » et condition de possession |
| Rampage, Provoke, Ingest, Dethrone, Training, Melee, Increment, Decayed | 9, 6, 6, 6, 5, 4, 5, 1 | grammaires de corps diverses (`sequence.for-each.action`, `choose.verb`, `effect.start`, …) |
| Exploit | 0 | la définition compile, mais l'événement « exploite » (CR 702.110b) n'est pas publié : politique d'événement manquante |

Hors périmètre (mécanismes moteur nouveaux), cartes bloquées seulement par
eux : Crew 74, Unearth 40, Madness 39, Partner 32 (règle de construction),
Start your engines! 29, Power-up 28, Visit 25, Exhaust 23, Spree 21,
Ninjutsu 14, Sneak 14, Encore 13, Myriad 13, Escape 12, Embalm 10, Banding
10, Blitz 9, Eternalize 9, Phasing 7.
