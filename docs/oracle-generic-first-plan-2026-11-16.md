# Plan d'attaque — parseur générique d'abord (16 novembre 2026)

Objectif : remplacer les recettes de phrase par des concepts Magic partagés
(cadres verbaux, sujets joueur/objet, requêtes d'objets, événements, conditions,
quantités), jusqu'à ce que ≈1 000 cartes étudiées couvrent les ≈39 000 cartes.
Complète `oracle-compositional-compiler.md` ; ne le remplace pas.

## Constat de départ

- Les déclencheurs passent déjà par le programme générique **avant** les familles.
  Les sorts, capacités activées et statiques passent par ≈128 familles
  `claims_promoted_*` **avant** le programme contextuel partagé.
- Plusieurs familles sont des recettes antérieures aux cadres partagés
  (ex. `triggered_attack_mill`, supprimée : le déclencheur générique + le verbe
  joueur `mill` la couvrent).
- Une famille trop gourmande bloque le chemin générique (Tormod's Crypt :
  `target player's graveyard` interceptée comme cible d'objet).

## Phase 1 — Crible des familles (mesure, puis suppression)

1. `python scripts/sieve-native-families.py --label sieve-N` : tout le corpus
   dédupliqué par `oracle_id`, deux passes (ordre normal / générique d'abord via
   `DEEPDECK_ORACLE_GENERIC_FIRST=1`), attribution par première opération.
2. Verdicts par famille :
   - **redundant** (IR identique partout) → supprimer la famille et son aiguillage ;
   - **mixed / divergent** → comparer les IR : si le générique est sémantiquement
     correct, migrer les attentes et supprimer ; sinon nommer le concept manquant ;
   - **needed** → concept partagé absent ; il entre dans la file classée de la phase 2.
3. Gains « newlyParsed » : capacités que le générique réussit mais qu'une famille
   faisait échouer → réordonner/restreindre la revendication de la famille.
4. Rejouer l'audit des dix derniers sets et le préfixe historique : 0 régression.

### Résultat `sieve-1` (16/11, 30 989 cartes, une par `oracle_id`)

- 14 familles **redondantes** (1 004 capacités, IR identique) → suppression en cours.
- Divergences : la plupart sont des noms d'identifiants (`targetGraveyardCard` →
  `targetCard`, préfixes de liaison du scry, déclaration vide vs absente).
  Deux vraies différences gardent leur famille : `deals X damage` (déclaration de
  X perdue) et `Its controller creates …` après exil (liaison LKI du contrôleur).
- 111 capacités **nouvellement reconnues** par le générique : des familles
  revendiquent puis échouent (`atomic-targeted-zone-change` sur « target card
  from a graveyard », rappels de mots-clés dans `static-collection-keyword`, etc.).
  Correctif sans risque d'IR : quand une famille échoue, essayer les parents
  partagés avant de rendre l'échec (patch préparé, appliqué après les suppressions).

Fin de phase : inverser l'ordre pour tous les types (générique d'abord) comme
pour les déclencheurs ; les familles restantes deviennent des repli explicites.

## Phase 2 — Concepts partagés classés par cartes débloquées

Mesurés sur les dix derniers sets (cartes débloquées seules) :

| Concept | Gain estimé | Nature |
|---|---:|---|
| `If you do` / `When you do` après action facultative | ≈44 cartes | programme : résultat d'action comme condition |
| Déclencheurs modaux (`choose one —`) | 27 | programme : modes sous un événement |
| `where X is EXPR` postposé | ≈26 | expression numérique liée |
| `as long as CONDITION` statique | 35 capacités | condition de statique continue |
| `each of up to N target` | ≈22 capacités (20 sets) | distributif sur cible plurielle |
| `until end of turn, …` antéposé | 15 | durée antéposée |
| sous-type singulier dans une collection | — | requête d'objets (`each other Goblin you control`) |

### Conception « If you do / When you do » (analyse du 16/11)

- Blocage mesuré : `program.optional.scope` (`effect_program.rs`) refuse toute
  suite après `you may …` — 125 « If you do » + 52 « When you do » sur 20 sets.
- Grammaire : `EffectProgramNode::Optional` gagne une continuation typée
  `{ IfPerformed | IfNotPerformed | WhenPerformed (réflexif, CR 603.12) }` dont le
  corps est un programme partagé ; l'antécédent (`that creature`, `it`) vient du
  résultat de l'action facultative.
- Moteur existant : `optionalAction { player, action, onPerformed }` et
  `createReflexiveTrigger { controller, ability }` (cibles choisies à la mise sur
  la pile). Manques : `onDeclined` pour `If you don't`, et la disponibilité de
  l'action est vérifiée seulement pour exileCards/discardCards/sacrificePermanents/
  exileAbilitySourceFromGraveyard (`_ => true` sinon). « If you do » doit dépendre
  de l'action réellement accomplie (CR 608.2c) : remplacer la liste blanche par
  un résultat d'exécution générique (`performed`) avant d'élargir.

**Conception retenue (16/11, après mesure).** Sur les 10 sets, l'action facultative
avant `If you do` / `When you do` est presque toujours un **coût** : payer du mana
(37), sacrifier (30), défausser (21), blight, payer des points de vie, retirer un
marqueur, engager. C'est donc un coût facultatif payé à la résolution :

```
you may COÛT_RÉSOLUTION . (If you do | If you don't | When you do) , PROGRAMME .
```

- `COÛT_RÉSOLUTION` : un lecteur partagé composé d'enfants existants — `pay` +
  coût de mana partagé, `pay N life`, `sacrifice` + requête d'objets
  (`a/an/another …`), `discard` + requête de cartes ; plus tard `tap`, marqueurs.
- Abaissement vers la primitive moteur existante
  `optionalPayCostPerformEffects { player, cost, effects, declineEffects }`
  (`cost` = `payMana{manaCost}` / `payLife` / `sacrificePermanent{where}` /
  `discardCard{where}` — schéma `CostProgram::decode`).
- `When you do` = `effects: [createReflexiveTrigger { ability }]` : le déclencheur
  réflexif garde ses propres cibles (CR 603.12), pour tous les coûts, au lieu de
  `optionalPayCostCreateReflexiveTrigger` limité à mana/engager/marqueurs.
- Les autres actions facultatives (`you may put … onto the battlefield`, `copy it`)
  restent un échec explicite jusqu'à un résultat d'exécution générique.

Lacune moteur notée : aucune production n'enregistre le lien « exilé avec cette
carte » (CR 607) ; Ghost Vacuum exile sans lien et sa seconde capacité ne trouve rien.

Boucle par concept (AGENTS.md) : grammaire partagée → IR primitive → exécution
moteur ; tests carte réelle + sœur + quasi-raté + runtime ; audit avant/après.

### Classement `after3` des 10 derniers sets (cartes débloquées seules, 16/11)

| Concept partagé | Cartes | Exemples d'état |
|---|---:|---|
| Issue d'une action facultative (`If you do` / `When you do` / `If you don't`) | ≈55 | `program.optional.scope`, `effect.start @ if you` |
| Événements manquants (`whenever you draw / gain life / cast / attack`, `one or more QUERY leave/enter/die/deal…` (sujet quantifié, voir ci-dessous), `a creature … deals combat damage`) | ≈60 | `unclaimed: whenever …` |
| Modes (`choose one —`) sous déclencheur/activation | ≈40 | `modal-declaration-timing`, `effect.start @ choose one` |
| `where X is EXPR` | ≈26 | `effect.start @ where x` |
| Rappel après action-mot-clé (`Surveil 1. (Look at …)`) | ≈18 | `activation.annotation.action`, `effect.start @ (look at` |
| `Power-up —` (TMT) | 18 | `activation.compound.cost @ power-up` |
| Durée antéposée `Until end of turn, …` | 15 | `effect.start @ until end` |
| Statique `as long as` | 12 | `unclaimed: as long as` |
| Coût `Exile this card from your graveyard` | 12 | `activation.compound.cost @ exile this` |
| `if this/that …` (condition intermédiaire) | ≈21 | `effect.start @ if this`, `@ if that` |

### Avancement du 16/11 (non commité)

| Étape | 10 sets (cartes complètes) | Corpus (capacités natives) | Régressions / IR modifiées |
|---|---:|---:|---|
| Départ de session | 437 | — | — |
| 13 familles redondantes supprimées (≈ −1 640 lignes) | 437 | 21 542 | 0 / 0 |
| Repli partagé quand une famille échoue | 438 | 21 632 | 0 / 0 |
| Événements joueur `ACTEUR VERBE` (gain/lose life, draw, attack, face a dilemma) | 444 | — | 0 / 0 |
| Limite `This ability triggers only once each turn.` (métadonnée) | 449 | — | 0 / 0 |
| En-tête modal `one or both` / `one or more` | 452 | — | 0 / 0 |
| Modes sous déclencheur (abaissement modal partagé spell/trigger) | 456 | — | 0 / 0 |
| `untap OBJET` sur le cadre verbal de `tap` | 466 | 21 949 | 0 / 0 |
| `you may COÛT. If you do / If you don't / When you do, …` (coût payé à la résolution) | 484 | +197 (crible 9) | 0 / 0 |
| Sujet quantifié + verbe d'événement (`leave your graveyard`, `attack`, `deal combat damage to a player`) | **487** | +49 (crible 10) | 0 / 0 (Old Man Willow : échec explicite voulu) |

### Régressions sémantiques corrigées (revue de l'agent de tests, 16/11)

- Les phrases de restriction (`Activate only …`, `This ability triggers only …`)
  n'arrêtent plus la séquence partagée que pour leur parent propriétaire
  (`AbilityRestrictionOwner` dans le contexte) ; les autres parents échouent
  explicitement au lieu de perdre la phrase. Zek et Meatsqueak gagnent leur
  `triggerLimit` ; Quakebringer (`triggers only if …`) échoue explicitement.
- X sans producteur : la famille d'échange de zone activé exige un coût `{X}`.
- Exil lié (CR 607) : une composition de carte rejette un exil sans relation
  source quand une capacité de la même face lit `cardsExiledWithSource`
  (Ghost Vacuum) — en attendant une primitive qui enregistre le lien.
- Rappels de mots-clés : la validation stricte coûterait 136 capacités officielles
  (schémas partagés incomplets) ; le parent statique reste souple (CR 207.2),
  décision laissée au propriétaire du projet.

Bilan net vs `sieve-5` : 2 capacités erronées deviennent des échecs explicites,
2 IR gagnent leur limite, rien d'autre ne bouge ; 10 sets toujours à 466.

### Coût facultatif et issue (16/11, implémenté)

- `automata/resolution_cost.rs` : `pay` (mana partagé | `N life`), `sacrifice a/an/another`
  + requête d'objets partagée, `discard a card` ; connecteurs `If you do` / `If you don't` /
  `When you do`. `EffectProgramNode::OptionalCost` ; l'issue ne gouverne que sa phrase
  (`program.optional-cost.scope` sinon).
- Abaissement : `optionalPayCostPerformEffects` ; `When you do` → `createReflexiveTrigger`
  dont l'aptitude porte ses propres cibles (CR 603.12). Moteur : `excludeSource` sur le
  coût de sacrifice à la résolution (`another`).
- +44 capacités sur les 10 sets (466 → 484 cartes complètes), 0 régression, 0 IR modifiée.
- Restent : coûts `blight`, `tap N untapped …`, `exile … from your graveyard`, `remove a
  counter`, coûts alternatifs (`sacrifice X or discard Y`), seconde issue (`If you do, …. If
  you don't, …` / `Otherwise`), antécédent du coût (`that creature's power`).

### Sujet quantifié des événements (décision du 16/11)

« one or more cards leave » n'est **pas** un type d'événement. L'événement est le verbe
(`leave`, `enter`, `die`, `attack`, `deal combat damage`, `are put into`, `become tapped`…)
avec ses zones ; le sujet est une composition partagée **borne numérique + requête d'objets** :

```
Événement := SUJET_QUANTIFIÉ VERBE [from/to ZONE] ...
SUJET_QUANTIFIÉ := (a | an | another) REQUÊTE_SINGULIÈRE
                 | NOMBRE "or more" REQUÊTE_PLURIELLE     (numérique partagé : one/1, two/2…)
```

- Le cadre **déclencheur** interprète la borne comme multiplicité (CR 603.2c) :
  `a/an` → une fois par objet correspondant dans l'événement ; `N or more` → une fois par
  événement contenant au moins N objets correspondants, l'ensemble lié (`those cards`).
- C'est l'**effet** qui définit l'événement : 3 cartes retirées par une même instruction =
  un événement (un déclenchement `one or more`) ; deux sorts retirant chacun 3 cartes = deux
  événements, deux déclenchements. Le moteur a besoin d'un identifiant de lot par événement
  de changement de zone (comme `batchId` pour les dégâts).
- Le cadre **condition** (`if one or more creatures died this turn`) interprète la même borne
  comme un simple compte ≥ N, sans regroupement.
- IR cible : `{kind: objectEvent, action, from?, to?, objects: REQUÊTE,
  multiplicity: {kind: perObject} | {kind: perEvent, minimum: N}}`.
- Dette à résorber : `GraveyardLeaveCounterTriggerNode` (`delve.rs`, événement + effet
  combinés), `recognize_positive_event_quantity` limité à 1 (`quantity_event.rs`), et les
  types moteur-recettes `cardsLeftGraveyard`, `smallControlledCreatureDied`,
  `opponentCreatureDied`, `controlledCreaturesCombatDamageToPlayer`,
  `tokenCreatedOrSacrificed`, `damageDealtToOpponent{aggregation}`.
- Mesure : les 29 déclencheurs `one/two/three or more` des 10 sets échouent tous
  (leave 8, deal 7, are put 5, attack 4, enter 3, die 1, become 1).
- **Implémenté (16/11)** : `automata/quantified_event.rs` (`EventMultiplicity`,
  `QuantifiedEventSubjectNode`, `ObjectEventNode`, accord du verbe en nombre), branché en
  dernier lecteur de l'enveloppe générique ; un sujet `N or more` engage le cadre (échec
  explicite sinon). Abaissement vers les lots moteur existants : `cardsLeftGraveyard`
  (`perCard` / `oneOrMore`), `controlledCreaturesAttacked{where}` (une déclaration
  d'attaque), `controlledCreaturesCombatDamageToPlayer{where}` (par joueur blessé).
  +10 capacités (484 → 487 cartes), 0 régression. Les 29 ne sont plus « unclaimed » :
  chacun a un diagnostic précis.
- Reste : identité de lot moteur pour `enter` / `die` simultanés (Mister Fantastic,
  Spiritcall Enthusiast, Great Fierce Bee, Twilight Diviner) ; verbes `are put into …
  from anywhere`, `become tapped`, `counters are put on` ; sujets joueurs (`one or more
  of your opponents`) ; seuil > 1 ; `other` sur un sujet pluriel ; requête
  `with +1/+1 counters on them`.
- **Corrigé (16/11)** — tête nominale partagée dans la requête d'objets : des modificateurs
  coordonnés (sous-type au singulier, couleur, négation) partagent la tête finale et ses
  qualificatifs postnominaux : « Ninja or Rogue creatures » = `and(or(Ninja, Rogue), Creature)`,
  « red or green creature with flying » = `and(and(or(red, green), Creature), flying)`. Une
  branche avec sa propre tête (type de carte, domaine, sous-type au pluriel, article répété)
  reste indépendante. Corpus : +3 capacités (Escape Routes, Evaporate, Rending Volley), 8 IR
  corrigées (Mindwrack Liege admettait toute carte bleue non-créature ; Grassland Crusader,
  Prosperous Thief, Kami of Restless Shadows…), 0 perte ; 10 sets inchangés.

### Ronde du 16/11 (soir) — lots d'événements, impératif, rappels

| Étape | 10 sets (cartes complètes) | Régressions / IR modifiées |
|---|---:|---|
| Tête nominale partagée (`Ninja or Rogue creatures`) | 487 | 0 / 0 (corpus : +3, 8 IR corrigées) |
| Identité d'événement simultané au moteur + `one or more … enter/die` | 489 | 0 / 0 |
| `mill` impératif (le contrôleur) | 490 | 0 / 0 |
| Texte de rappel après une phrase complète (CR 207.2) | 527 | 0 / 0 |
| Parties Aventure / Présage / Préparé rejetées (pas de moteur CR 715) | **504** | 51 rejets voulus (faux positifs) |

- Moteur : `engine/event_batch.rs`. Une instruction d'effet, ou une passe d'actions basées sur
  l'état (CR 704.3), est un événement ; un déclencheur `aggregation: oneOrMore` ne se
  déclenche qu'une fois par source, règle et événement.
- Validation (agent, copie isolée) : crible `sieve-12` +339 capacités (225 rappels, 84 mill,
  30 one-or-more), 49 rejets voulus Aventure/Préparé, 0 IR modifiée ; tests moteur
  inchangés avec/sans lots sauf le test ciblé. Corrigé ensuite : `forEachPlayer` partage
  un événement par instruction-enfant entre tous les joueurs (CR 101.4).
  À surveiller : Wrath of Sod (carte de test, compteur défini seulement par son rappel),
  « (Activate only as an instant.) » de Chromatic Sphere et des Œufs ignoré comme rappel.
- Rappels : une parenthèse équilibrée après une phrase complète est ignorée, sauf quand un
  lecteur typé la revendique (scry, jeton nommé) ou qu'un parent possède l'annotation.
- Honnêteté de couverture : 19 cartes Aventure/Préparé étaient déjà comptées complètes sans
  moteur de layout. Prochain gros gain : implémenter CR 715 (Aventure : exil à la résolution
  et permission de lancer la créature) puis Présage et Préparé — 51 capacités sur les 10 sets.

## Phase 3 — Dette structurelle

- Ajouter une action oblige ≈10 `match` exhaustifs dans 7 fichiers : regrouper
  les propriétés d'action (sujet permanent, cibles, antécédents, effet sur la
  bibliothèque) derrière une description unique par nœud.
- Remplacer les vérifications de forme dans `classifier.rs` (doublons des
  revendications) par la reconnaissance structurelle.

## Organisation

- Une seule voie Cargo ; la validation (tests, partitions CI, audits) tourne dans
  un agent en arrière-plan pendant que l'analyse/conception continue.
- Rien n'est commité sans demande explicite ; préserver les fins de ligne.
