# File de tâches déléguées — couverture Oracle (16 novembre 2026)

File de tâches bornées qu'un modèle moins coûteux peut implémenter sous orchestration.
L'orchestrateur choisit la tâche, relit le diff, mesure, accepte ou renvoie.
Mesures de départ : audit `after19` (10 derniers sets, 504 cartes complètes) et crible `sieve-11`.
Les nombres « cartes » comptent les cartes dont c'est la **seule** capacité en échec.

## Protocole commun (à lire avant toute tâche)

1. Lire `AGENTS.md`, `docs/oracle-compositional-compiler.md` et
   `docs/oracle-generic-first-plan-2026-11-16.md`.
2. **Concept, pas recette.** Étendre un enfant partagé (requête d'objets, quantité,
   coût, verbe, événement, programme). Interdit : reconnaisseur nommé d'après une carte,
   phrase complète codée en dur, primitive combinée (`dealAndGainLife`), regex.
3. **Pas d'approximation.** Une forme non représentable échoue explicitement avec un
   état d'automate précis. Ne jamais accepter une capacité en jetant une partie du texte.
4. **Vérifier le moteur avant d'émettre une IR.** Chaque champ émis doit être lu par le
   moteur (`src/engine`) et accepté par sa fonction `*_supported`. Un champ ignoré par le
   moteur est une approximation silencieuse.
5. **Fichiers.** N'éditer que les fichiers listés « possédés » ; préserver les fins de ligne
   (CRLF/LF) ; `rustfmt --edition 2024 --config skip_children=true <fichiers>` seulement.
   Ne jamais committer, réinitialiser, ni toucher au travail d'autres agents.
6. **Une seule voie Cargo** : `CARGO_TARGET_DIR=.tmp/targets/card-migration`. Ne pas créer
   d'autre cible (le disque D: est presque plein).
7. **Tests** : un test carte réelle, un test sœur (combinaison nouvelle du même concept), un
   quasi-raté (échec explicite attendu), dans un fichier `*_tests.rs` dédié.
8. **Portes d'acceptation — mode rapide (depuis le 16/11 soir)** : l'agent lance seulement
   les tests ciblés de son concept (`cargo test -p deepdeck-oracle --lib -- <filtre>`) et
   l'audit des 10 sets (gains, 0 régression, 0 IR modifiée). La suite lib complète, la
   compilation moteur et le crible du corpus sont lancés par l'orchestrateur à chaque point
   de contrôle (toutes les 2 à 4 tâches). Portes complètes d'origine, pour référence :
   - `cargo test -p deepdeck-oracle --lib` : seuls les 5 échecs connus
     (`compact_mana_envelope…`, `delegates_death_event…`, `imperative_life_action…`,
     `default_preserves_native_phase_failure…`, `third_person_life_verb…`).
   - `python scripts/audit-latest-sets.py --label <id-tâche> --compare-to after19` :
     gains ≥ 1, **0 régression, 0 IR modifiée** (toute IR modifiée doit être justifiée
     carte par carte comme correction).
   - Si le moteur change : `cargo build -p deepdeck-engine-runtime --lib` réussit, et un test
     d'exécution est écrit dans `src/engine/tests/` (il sera lancé par l'orchestrateur en
     copie isolée, le jeu de tests moteur ne compilant pas dans l'arbre partagé).
9. **Remise** : fichiers modifiés, concept ajouté (grammaire en une ligne), tests et
   résultats, chiffres de l'audit, cas laissés en échec explicite et pourquoi.

## Tâches prêtes (classées par cartes débloquées / risque)

### T1 — Rappel typé de `surveil` · ≈23 cartes · petit · Sonnet
- **Constat** : `Surveil 1. (Look at the top card…)` échoue à `effect.start @ (` (12) et,
  dans une capacité activée, à `activation.annotation.action @ look` (11). Le rappel de
  `scry` a un lecteur typé (`scry_reminder_starts_at`, `recognize_scry_reminder`) qui
  bloque aussi le saut générique quand la dernière action est `surveil`.
- **À faire** : valider le rappel de surveil avec le même lecteur typé (paramétré par
  l'action : scry → « on the bottom », surveil → « into your graveyard »), dans la séquence
  et dans l'annotation d'activation.
- **Exemples** : Shattered Wings, Starfighter Pilot, Fields of Strife, Forum of Amity.
- **Quasi-raté** : `Scry 1, then surveil 2. (Look at … on the bottom.)` reste un échec.
- **Fichiers** : `automata/scry*.rs`, `automata/sequence.rs` (branche rappel seulement),
  `automata/activated_effect_sequence.rs` (annotation).

### T2 — Jeton créé engagé · ≈13 cartes · petit · Sonnet
- **Constat** : `create a tapped 2/2 colorless Robot artifact creature token` échoue à
  `token.named.catalog-identity @ tapped`.
- **À faire** : l'état d'entrée (`tapped`, `tapped and attacking` si déjà connu ailleurs)
  est un modificateur du nom du jeton, lu avant la description ; abaisser vers le champ
  que le moteur lit déjà pour `createTokens` (vérifier `effect_domains`, sinon ajouter le
  champ au moteur avec son test d'exécution).
- **Exemples** : Gravpack Monoist, Melded Moxite, Sela (Romulan token).
- **Fichiers** : `automata/token*.rs`, `effects/token*.rs`, éventuellement le domaine
  moteur de création de jetons.

### T3 — Coût d'activation « Discard a card » · ≈9 cartes · petit · Sonnet
- **Constat** : `Discard a card: …` échoue à `cost.discard-self.reference @ a` : le lecteur
  de coût d'activation ne connaît que `Discard this card`.
- **À faire** : ajouter `discard a/one card` (et `discard a QUERY card` si la requête de
  cartes en main existe) à `recognize_activation_cost` en réutilisant le nœud de
  `automata/resolution_cost.rs` ; abaisser vers `discardCard { where }`, déjà payé par le
  moteur pour les coûts d'activation (vérifier `cost_supported`).
- **Exemples** : Iron-Shield Elf, Murmuring Volume.
- **Fichiers** : `automata/compound_activated.rs`, `abilities/activated_native.rs`.

### T4 — Déclencheur source `enters or dies` · ≈12 cartes · petit · Sonnet
- **Constat** : `When this creature enters or dies, …` échoue à
  `trigger.source-program.separator @ or`.
- **À faire** : l'enveloppe source accepte une union de verbes de transition ; abaisser en
  `{ "kind": "oneOf", "events": [enterBattlefield self, permanentDied self] }` (forme déjà
  utilisée par les transitions de permanent).
- **Exemples** : Lys Alana Informant, Noggle Robber.
- **Quasi-raté** : `enters or attacks` reste explicite tant que l'union n'est pas prouvée.
- **Fichiers** : `automata/source_trigger_envelope.rs`, `effects/trigger_event.rs`.

### T5 — Multiplicateur `for each` en fin d'action · ≈9 cartes · moyen · Sonnet
- **Constat** : `draw a card for each color among …`, `add {G} for each charge counter on
  this enchantment` échouent à `sequence.connector-or-end @ for`.
- **À faire** : `for each QUERY` multiplie la quantité de l'action précédente par un compte
  partagé (`countPermanents`, `countCounters`…) ; réutiliser `read_optional_for_each_count`
  de `effects/static_continuous.rs` en le déplaçant vers un enfant numérique partagé.
- **Exemples** : Gardenize, Karn, Gilded Guardian.
- **Fichiers** : `automata/numeric*.rs` (nouvel enfant), `automata/player_action.rs`,
  `automata/mana*.rs`, `effects/static_continuous.rs` (réutilisation).

### T6 — Mot-clé `Changeling` · ≈9 cartes · petit · Sonnet
- **Constat** : `Changeling (This card is every creature type.)` n'est revendiqué par aucun
  automate ; le moteur connaît déjà `changeling`.
- **À faire** : ajouter le mot-clé à la liste de mots-clés partagée, avec son rappel validé
  par le schéma de rappel existant ; abaisser vers la forme que le moteur lit.
- **Exemples** : Changeling Wayfinder, Graveshifter.
- **Fichiers** : `automata/keyword*.rs`, `effects/keyword*.rs`.

### T7 — Réduction de coût conditionnelle · ≈18 cartes · moyen · Sonnet
- **Constat** : `This spell costs {1} less to cast if you control a Kithkin.` échoue à
  `casting-reduction.terminator @ if`.
- **À faire** : la réduction accepte une condition partagée (`BooleanConditionNode`,
  présence/compte de permanents) ; le moteur doit évaluer la condition au calcul du coût
  total (CR 601.2f) — vérifier le champ attendu par le moteur, sinon l'ajouter avec test.
- **Exemples** : Mistmeadow Council, Lashwhip Predator.
- **Fichiers** : `automata/casting_reduction*.rs`, `effects/static_spell_reduction.rs`,
  moteur : calcul des coûts (`casting_cost*`) seulement si nécessaire.

### T8 — Coût d'activation « Exile this card from your graveyard » · ≈17 cartes · moyen · Sonnet
- **Constat** : `{W}, Exile this card from your graveyard: …` échoue à
  `activation.compound.cost @ exile`.
- **À faire** : nouveau coût d'activation « exiler la source » dont la zone d'activation est
  le cimetière (`activationZone: "graveyard"`, déjà lu par le moteur pour d'autres zones) ;
  vérifier que le moteur propose les capacités activées depuis le cimetière.
- **Exemples** : Goldmeadow Nomad, Stoic Grove-Guide.
- **Fichiers** : `automata/compound_activated.rs`, `abilities/activated_native.rs`,
  moteur : activation depuis la zone (test d'exécution obligatoire).

## Avancement (16/11 soir)

T1–T24, C1, C2, Q1–Q4, Q3b, R1, R2, I1, O1 acceptés et intégrés : audit `o1-merged` = **733/2554** ; corpus entier `sieve-18` = **7987/35353 (22,6 %)** cartes complètes
(504 au départ de la file), 0 régression, 0 IR modifiée ; suite lib = 1115 réussis + les
5 échecs connus (1122 réussis après T13/T14) ; `cargo build -p deepdeck-engine-runtime --lib` réussit.
Orchestrateur :
- un coût facultatif peut ouvrir une phrase ultérieure et ne gouverne que sa phrase de
  résultat (variante `EffectProgramNode::Steps`, CR 608.2c) ;
- `blight N` est un coût de résolution payable (`putCounters -1/-1` sur une créature
  contrôlée ; impayable sans créature, donc « If you do / If you don't » exacts).
Dette : les tests d'exécution moteur de T7 (`conditional_casting_reduction_runtime`) et T8
(`graveyard_source_activation_runtime`) n'ont pas été exécutés : le crate de tests moteur
ne compile pas dans l'arbre partagé (≈21 erreurs préexistantes dans `src/engine/tests.rs`).
Idem pour T13 (`target_conditional_casting_reduction_runtime`) et T14 (deux tests dans
`token_runtime.rs`).

## Vague 2 (classées par cartes débloquées)

### T9 — Action-mot-clé `Empower Jace N` · ≈15 cartes · moyen
- **Constat** : `empower Jace 2. (Put two loyalty counters on a Jace token you control. If
  you don't control one, first create …)` échoue à `reference.source.unresolved-name`.
- **À faire** : définition autoritaire = texte de rappel imprimé (pas de règle CR connue) ;
  action-mot-clé typée qui se développe en primitives existantes (création de jeton du
  catalogue si absent, puis marqueurs de loyauté sur le jeton choisi). Copie `stage-t9`.

### T13 — Réduction de coût `if it targets …` · ≈12 cartes · petit
- Suite de T7 : le moteur a un crochet `targetWhere` pour les réductions propres.
  Copie `stage-t4`.

### T14 — Jetons nommés du catalogue (Lander, Mutagen, Heartwood…) · ≈16 cartes · petit
- `token.named.catalog-identity @ lander` : ajouter les identités au catalogue partagé
  depuis les données de cartes locales (pas de texte inventé). Copie `stage-t5`.

Point de contrôle `t15-merged` : suite lib 1151 réussis + 5 connus ; moteur compile ; crible
`sieve-13` stable (2 pertes générique-d'abord, identiques à `sieve-12` : X d'un coût additionnel).
T9 Empower Jace : action gardée tant que le jeton du catalogue ne se compile pas entièrement
(pas d'approximation) ; ouverte par T19 (capacités de loyauté génériques) et par le rappel de
surveil `that card` (ancienne formulation) → +27 capacités. En cours : T17 `as long as`,
T18 `Until end of turn,` antéposé.

### Suivantes (non lancées)
- Suites de T11 : `it` lié au jeton qui vient d'être créé (≈3 cartes, le moteur a déjà
  `createTokens.bind`) ; destination = objet entrant observé ; la légalité d'attachement
  lit le type imprimé, pas les caractéristiques effectives ; restrictions d'enchant des Auras.
- `blight N` comme coût facultatif à la résolution (≈6) — orchestrateur (`resolution_cost`).
- `tap N untapped CREATURES you control` comme coût (résolution et activation, ≈5).
- `attach it to target …` (≈10) ; `Basic landcycling {N}` (≈9) ;
  `This creature enters with N counters` (≈5 cartes, 29 capacités).

Q1 intégré (lignes de rappel seules, rappels de mots-clés) : corpus 6850 → 7036 cartes
complètes. `scripts/audit-latest-sets.py` compare désormais par texte de capacité (et rang
d'occurrence) au lieu de la position : une ligne de rappel retirée ne décale plus les clés, et
une capacité native disparue compte comme régression.

Dette T24 : la valeur `that many`/`that much` d'un événement est portée par le curseur
(`machine.rs`) ; elle devrait vivre dans `EffectRecognitionContext` comme liaison typée.

## Conception : un coût est une suite d'actions ordinaires (demande de l'utilisateur, 16/11)

Un coût n'a pas de grammaire propre : `pay {2}`, `sacrifice a creature`, `discard a card`,
`tap two untapped artifacts you control`, `exile this card from your graveyard`, `blight 1`,
`remove a counter`, `return a land you control to its owner's hand` sont les **mêmes actions**
que dans un effet, lues par les **mêmes lecteurs** (verbe + sélection partagée + quantité).
Seul le parent change :

| Parent (contexte de coût) | Forme | Sémantique ajoutée par le parent |
|---|---|---|
| Activation | `A, B: effet` | payé à l'activation (CR 602.2b) |
| Coût additionnel | `As an additional cost to cast this spell, A [or B]` | payé au lancement (601.2f-h) |
| Coût alternatif | `You may A rather than pay this spell's mana cost` | remplace le coût de mana |
| Coût facultatif à la résolution | `you may A. If you do / If you don't / When you do` | 608.2c, 603.12 |
| `unless` / ward | `… unless PLAYER A` ; `Ward—A` | payeur désigné |

Le parent fixe le **payeur**, l'**obligation de tout payer** (CR 118.3 : un coût impossible ne
peut pas être payé), les **choix au paiement** (pas des cibles, 601.2h) et la **zone de la
source** déduite des actions (`discard this card` → main, `exile this card from your
graveyard` → cimetière, `{T}`/`sacrifice this` → champ de bataille).

État : ≈25 lecteurs `recognize_*_cost` et deux nœuds (`LocalActivationCostNode`,
`ResolutionCostNode`) dupliquent les lecteurs d'action. Cible : un `PaymentActionNode`
produit par les lecteurs d'action partagés, une `CostSequenceNode` (liste + `or`), et un
abaissement unique par schéma moteur. Étape 1 (analyseur, IR inchangée) ; étape 2 (moteur :
un seul schéma de paiement pour activation, additionnel et résolution).

## Conception : phrase conditionnelle `If CONDITION, EFFETS.` (≈567 cartes, corpus entier)

Mesure `sieve-14` (871 capacités, 567 cartes à blocage unique) par forme de condition :
état d'un objet référencé (`If it's a land card`, `If that creature is a Vampire`, `If its
mana value was 3 or less` : 174 cartes) ; faits du lancement (`kicked`, `cast from …`,
`mana spent` : 82) ; contrôle (`If you control …` : 81) ; résultat (`… this way` : 55) ;
historique du tour (41) ; `instead` (27) ; état d'un joueur (9).

- **Cadre** : le parent séquence lit `If COND, EFFETS.` comme une phrase conditionnelle,
  suivie facultativement de `Otherwise, EFFETS.` ; abaissement `conditional{condition, then,
  else}` (moteur : `effect_domains/control.rs`). La condition gouverne sa phrase seulement.
- **Condition** : programme booléen partagé (`and`/`or`/`not`) dont les atomes viennent des
  lecteurs existants : gardes de contrôle (`recognize_trigger_guard_atom`), atomes d'état et
  d'historique de `static_condition.rs` (T17), faits du lancement (`wasKicked`,
  `wasCastFromHand`, `wasCastFromZone`), et un nouvel atome **objet référencé + prédicat
  partagé** (`it`/`that NOUN`/`its PROPERTY` résolu par la mémoire d'anaphore de la séquence,
  jamais deviné).
- **Moteur** : `objectMatchesFilter` n'accepte que source ou cible avec un filtre de
  permanent ; il faut l'étendre aux objets liés (carte révélée, meulée, choisie) avec un
  filtre de carte, et à une propriété numérique comparée.
- **Hors périmètre de la première passe** : `instead` (remplacement de la phrase précédente),
  `… this way` (résultat d'action, demande un compte d'événements), comparaisons entre joueurs.

## Réservées à l'orchestrateur (conception transversale)

| Concept | Cartes | Pourquoi pas délégué |
|---|---:|---|
| Moteur Aventure / Présage / Préparé (CR 715) | 23 + 51 capacités | nouveau mécanisme de lancement, zones et permissions |
| Conditions intermédiaires `If …` | ≈66 | familles hétérogènes (remplacement, résultat d'action, état) |
| `where X is EXPR` | ≈27 | liaison numérique postposée sur toute la séquence |
| `blight N` comme coût à la résolution | ≈23 | coût avec choix d'objet et marqueurs -1/-1 |
| Durée antéposée `Until end of turn, …` | ≈20 | portée sur plusieurs actions |
| `choose …` (choix d'objets, modes conditionnels) | ≈23 | décisions déclarées vs résolution |
| `Power-up —` | ≈18 | mécanique à définir depuis les règles |
| Modèle d'événement unifié, IR typée au chargement | — | refonte d'architecture |
