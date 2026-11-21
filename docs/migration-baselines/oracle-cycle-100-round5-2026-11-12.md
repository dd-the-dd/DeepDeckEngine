# Cycle 5 — décès de permanents et séquence d'effets

Blood Artist et Pitiless Plunderer sont maintenant couvertes de bout en bout
par le même parent déclenché `permanentDied`. La reconnaissance est fondée sur
les tokens et des sous-automates typés; elle n'ajoute ni regex, ni phrase
complète, ni branche portant le nom d'une carte.

Le snapshot final de ce cycle est
`0F5B240B45AFDBFA0D8FCB69412ACA25EBC52595528E24AEE2686269FF4532BF`.
Il contient aussi le chantier parallèle `additionalLandPlay`. Les gains des
deux concepts sont donc séparés ci-dessous. Seules Blood Artist et Pitiless
Plunderer sont promues par ce rapport; Exploration attend encore sa preuve
runtime et ne doit pas encore servir à regarnir la file.

## Capacités et IR primitives

Blood Artist :

> Whenever this creature or another creature dies, target player loses 1 life
> and you gain 1 life.

Le parent événementiel conserve explicitement l'union entre la source et une
autre créature. Il ne réduit pas la phrase à « n'importe quelle créature », car
la source doit encore déclencher la capacité si elle perd le type Creature
avant de mourir. L'IR est une `oneOf` de deux événements `permanentDied`, puis
une déclaration `chooseTargets(players)` et deux effets ordinaires et ordonnés :
`loseLife(chosenTarget, 1)`, puis `gainLife(controllerOf(self), 1)`.

Pitiless Plunderer :

> Whenever another creature you control dies, create a Treasure token.

L'événement partage le prédicat `cardTypeContains(Creature)`, le contrôleur de
la source et `excludeSource`. L'effet produit le primitif générique
`createTokens` avec un jeton de catalogue nommé `Treasure`; aucun effet
« Pitiless » ou composite n'existe dans le moteur.

Les principaux états du parent sont : déclencheur, sujet de l'événement,
alternatives répétables séparées par `or`, contrainte de contrôleur,
`dies`, puis délégation à `EffectSequence`. La séquence retient les références
de joueur comme mémoire transitoire, mais choisit la famille d'action au verbe :
`gains`/`loses` route vers les effets de vie, tandis que `mills` reste un effet
de meule indépendant.

## Comportement simultané du moteur

Le moteur collectait auparavant les permanents létaux en un lot, mais les
retirait et créait leurs déclenchements un par un. Une source morte au début du
lot pouvait donc manquer les morts suivantes. Le correctif partagé capture les
sources, règles et informations LKI du lot avant le premier déplacement, puis
crée les déclenchements à partir de ce snapshot.

Trois scénarios comportementaux prouvent les deux formes : morts simultanées
par actions basées sur un état, morts simultanées après dégâts, et création du
Treasure lorsque Pitiless Plunderer meurt dans le même lot qu'une autre
créature. Le correctif porte sur la collecte des déclenchements de décès; il ne
prétend pas résoudre à lui seul toutes les interactions de remplacement ou de
destruction simultanée dépendant d'effets statiques.

## Preuves verticales

- Parseur final NativeOnly : 103/103 cas, dont les exactes Blood Artist et
  Pitiless Plunderer, deux siblings génériques, un near-miss structuré, la
  restauration de Restless Reef et huit cas `additionalLandPlay`.
- Contrats : les deux règles exactes sont primitives et exécutables.
- Runtime ciblé : quatre fixtures, 29 assertions, toutes vertes.
- Runtime élargi : 342/342 fixtures et 2 118 assertions vertes.
- Égalité : IR parseur = IR contrat = IR embarquée dans toutes les fixtures
  runtime des deux cartes. Hash Blood Artist :
  `4672EBEC0A72E639848D8B0E4D6A9949F1547000989C47BF5C31E9D7AB07CA2D`;
  hash Pitiless Plunderer :
  `3052F23B4CD37ACB31513D34057200B643A04130A0C12F210FA75D16E7F53811`.
- Les branches exactes historiques de ces deux capacités ont été retirées; le
  chemin promu n'utilise aucun fallback de carte.

Une première version routait trop tôt toute référence `target player` vers les
effets de vie. Le gate global a détecté Restless Reef. Le dispatch observe
désormais le verbe avant de choisir l'enfant. Sa capacité « Whenever this land
attacks, target player mills four cards. » reste native et produit exactement
`mill(chosenTarget(targetPlayer), 4)` avant et après le cycle.

## Audit comparatif immuable

Le comparateur protège l'identité, l'ordre et le texte de toutes les capacités,
ainsi que le type, l'IR et les diagnostics de chaque capacité auparavant
native. Le snapshot stable antérieur est `9DFFE7AA…`; le snapshot rouge
intermédiaire `DA872894…` est conservé comme preuve de la collision Restless
Reef, mais n'est pas utilisé comme baseline finale.

| Corpus | Capacités antérieures préservées | Nouvelles capacités natives | Nouvelles cartes entièrement natives | Régressions |
|---|---:|---:|---:|---:|
| Lot round 5, 100 cartes | 26/26 | 4 | 3 | 0 |
| File rotative, 450 cartes | 242/242 | 5 | 3 | 0 |
| Sept ensembles, 2 069 cartes | 1 121/1 121 | 5 | 1 | 0 |
| Global local, 39 288 cartes | 14 369/14 369 | 43 | 13 | 0 |

Sur le corpus global, les 43 nouvelles capacités se répartissent ainsi :

- 17 capacités de décès sur 16 cartes;
- 4 séquences réutilisant la création de jetons nommés;
- 22 capacités du chantier parallèle `additionalLandPlay`/ses variantes.

Les neuf cartes entièrement natives attribuables à cette tranche
`permanentDied` et à son lowering partagé sont Blood Artist, Falkenrath Noble,
Moldervine Reclamation, Many Partings, Pitiless Plunderer, Shelob's Ambush,
Moonlit Wake, Savor et Reckless Ransacking. Les quatre autres gains complets du
snapshot combiné appartiennent au chantier de terrain additionnel : Titania,
Exploration, Azusa, Lost but Seeking et The Explorer. Les gains globaux
mesurent la reconnaissance et l'IR; les nouvelles preuves runtime verticales
de ce rapport restent limitées aux deux cartes de décès ciblées.

## Temps mesurés

Le tracker a commencé après une partie du préaudit. Les 13 secondes d'analyse
ci-dessous excluent donc explicitement ce travail antérieur. La grande fenêtre
d'implémentation comprend du parking réversible, de la coordination, de la
revue statique et l'application atomique; elle ne représente pas 976 secondes
de frappe active. Le tracker affiche deux « boucles », mais la première est un
addendum de chronologie : une seule vraie boucle `preuve -> diagnostic -> edit`
a ajouté le lowering générique des jetons nommés.

| Étape chronologique | Temps | Description |
|---|---:|---|
| Analyse mesurée | 13,225–13,244 s | IR, FSM, contrats et risque de décès simultanés |
| Attente d'intégration | 100,054–100,057 s | Fichiers partagés occupés; aucun edit source |
| Fenêtre implémentation/staging | 976,467–976,535 s | Code, parking réversible, revue et intégration atomique |
| Build combiné, essai 1 | 37,114 s | Échec E0618; aucun runner runtime frais produit |
| Rebuild parseur | 35,286 s | Correction du masquage `controller_scope` |
| Relink runtime distinct | 2,672 s | Nécessaire après l'échec du build combiné |
| Runtime ciblé | 0,735 s | 4 fixtures, 29 assertions |
| Régression runtime | 1,170 s | 342 fixtures, 2 118 assertions |
| Checkpoint parseur intermédiaire | 14,040 + 0,639 s | 93/93, snapshot DA87 avant correction Restless |
| Checkpoint parseur final combiné | 30,523 + 0,645 s | 103/103, snapshot 0F5B |
| Liaison directe de l'analyseur | 1,107 s | `rustc` contre le rlib frais, sans Cargo |
| Audit lot round 5 | 0,502 s | 100 cartes, 201 capacités |
| Audit file 450 | 0,545 s | 450 cartes, 837 capacités |
| Audit sept ensembles | 2,545 s | 2 069 cartes, 4 300 capacités |
| Audit global | 28,769 s | 39 288 cartes, 67 420 capacités |

## Artefacts

- égalité parser/contrat/runtime :
  `.tmp/permanent-died-parser-contract-runtime-equality.json`;
- audit global final : `.tmp/oracle-global-nativeonly-0F5B240B.jsonl`;
- comparaison globale : `.tmp/oracle-global-9DFF-to-0F5B-comparison.json`;
- comparaison du lot :
  `.tmp/rotating-next-100-round5-9DFF-to-0F5B-comparison.json`;
- comparaisons file et ensembles :
  `.tmp/rotating-migration-queue-450-9DFF-to-0F5B-comparison.json` et
  `.tmp/seven-local-sets-9DFF-to-0F5B-comparison.json`;
- timings des cartes :
  `.tmp/card-migration-timings/round4-blood-artist.json` et
  `.tmp/card-migration-timings/round4-pitiless-plunderer.json`;
- audit rouge conservé : `.tmp/oracle-global-9DFF-to-DA87-comparison.json`.

Le lot suivant ne doit être regarni qu'après la preuve runtime du chantier
`additionalLandPlay`, même si le gate de non-régression du snapshot combiné est
déjà vert.
