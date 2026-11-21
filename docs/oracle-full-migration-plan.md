# Plan de migration complète du compilateur Oracle (20 novembre 2026)

Objectif : que chaque carte Magic soit lue par le compilateur généralisé et
exécutée correctement par le moteur, par des concepts partagés (cadres verbaux,
requêtes d'objets, quantités, événements, conditions, coûts vus comme des
actions), sans recette par carte, sans regex et sans approximation silencieuse.
Ce plan ordonne le travail ; `oracle-compositional-compiler.md` reste le
contrat d'architecture et `oracle-migration-objective.md` l'objectif.

## Mesure

Une carte est **supportée** quand chacune de ses capacités est `canonical`,
`executable`, avec 0 règle non exécutable — la sémantique de `POST /oracle/rules`
utilisée par le site. Deux sondes, mêmes requêtes, une carte par `oracle_id` :

- `examples/league_support_probe.rs` sur `.tmp/released-vs-main/league-input.jsonl`
  (1 664 cartes jouées en ligue) et `corpus-input.jsonl` (35 770 cartes) ;
- `scripts/rank-oracle-blockers.py <probe.jsonl>` classe les états de l'analyseur
  par cartes bloquées **seules** et donne le cadre des capacités non revendiquées.

| Point de mesure | Ligue (1 664) | Corpus (35 770) |
|---|---:|---:|
| v0.1.6 publiée (repli regex, exécution non prouvée) | 1 474 | 13 830 |
| Compilateur généralisé, `oracle/generalized-compiler` | 678 | 10 262 |
| + #27 mots-clés (mesuré seul) | 692 | 10 371 |
| + #28 contrôle, copie, joueurs (mesuré seul) | 692 | 10 509 |

**Constat central** : sur 25 508 cartes non supportées, **17 914 n'échouent que
sur une seule capacité**. Chaque concept partagé corrigé convertit directement
des cartes entières ; l'effort se classe donc par cartes bloquées seules.

### Blocages actuels (corpus, cartes bloquées seules / touchées)

| État de l'analyseur | Seules | Touchées | Nature |
|---|---:|---:|---|
| `unsupported_oracle_ability` | 4 279 | 8 911 | aucun cadre ne revendique la capacité |
| `effect.start` | 1 708 | 2 738 | verbe ou cadre de clause inconnu |
| `reference.source.unresolved-name` | 1 035 | 1 967 | surtout des verbes inconnus lus comme des noms |
| `numeric.atom.start` | 704 | 1 229 | quantité manquante |
| `sequence.connector-or-end` | 692 | 1 030 | connecteur ou forme de phrase |
| `selector.predicate.start` | 686 | 1 128 | prédicat d'objet ou de joueur |
| `target-list.*` | 646 | 926 | formes de cibles |
| `effect.life.action`, `effect.damage.action` | 557 | 1 054 | sujets et verbes de vie / blessures |
| `trigger.*`, `event.*` | ≈700 | ≈1 100 | événements de déclenchement |
| `condition.*` | ≈260 | ≈410 | atomes de condition |
| `keyword-list.item`, `selector.keyword-qualifier.*` | 334 | 605 | vocabulaire des mots-clés |
| `token.named.catalog-identity` | 224 | 399 | jetons nommés, copies de jetons |
| `card-structure.layout-runtime` | 82 | — | dispositions sans exécution |

Capacités non revendiquées (4 279 cartes) par cadre structurel : texte de sort
ou mot-clé 1 750, déclencheur 1 524, remplacement ou statique 845, activée 160.

## Paliers de périmètre

- **A — cartes jouées** : ligue DeepDeckLeague puis formats tournoi (Legacy,
  Commander, Modern, Pioneer, Standard). Condition du redéploiement du site.
- **B — tout le papier** : le reste du corpus imprimé.
- **C — produits spéciaux** : Schemes (`when you set …`), Planes, Attractions et
  Contraptions (`crank`, `visit`), Un-sets, cartes numériques Alchemy. Traités
  en dernier, avec leurs propres zones et objets hors jeu.

## Chantiers

Chaque chantier est une suite de **PR par thème** (une PR = un concept partagé
et ses preuves). Les estimations sont des cartes bloquées seules dans le corpus,
mesurées sur la sonde du 18 novembre ; elles se recouvrent partiellement.

| # | Chantier | Cible | Dépendances |
|---|---|---:|---|
| C0 | **Atterrissage** : CI verte de #26 (formatage, partitions, quarantaine explicite des manques de grammaire), fusion de #26 puis #27/#28 | — | aucune |
| C1 | **Revendication structurelle universelle** : chaque capacité est revendiquée par son cadre (déclencheur, activée, statique, remplacement, sort, mot-clé) et échoue à l'intérieur avec un diagnostic précis ; plus aucun `unsupported_oracle_ability` | ≈4 300 | C0 |
| C2 | **Verbes et actions-mots-clés** : catalogue CR 701 complet (explore, connive, investigate, amass, adapt, manifest, populate, venture, goad, surveil…), verbes de zone et de jeu manquants | ≈2 700 | C1 pour les verbes dans les cadres revendiqués |
| C3 | **Événements** : attaques et blocages (source, autre, joueur), unions d'événements, transitions d'état (devient engagé, bloqué), sorts lancés avec ordinal, pioche de la n-ième carte, cyclage, défausse, compteurs placés | ≈1 500 | C1 |
| C4 | **Quantités et réutilisation des résultats** : « for each … this way », « the number of », totaux, maxima, « that much », X lié | ≈1 400 | — |
| C5 | **Références et anaphores** : that player / that creature's controller, it après déplacement, sujets nominaux, noms courts | ≈700 | — |
| C6 | **Sélecteurs et prédicats** : prédicats de joueur, d'objet, qualificatifs de mots-clés, « with/without », relations de contrôle | ≈1 000 | — |
| C7 | **Séquence, portée et cibles** : connecteurs, portée des facultatifs, formes de listes de cibles | ≈1 500 | — |
| C8 | **Conditions** : atomes de condition de phrase, intervening-if, « as long as » | ≈400 | — |
| C9 | **Sous-systèmes de règles du moteur** (exécution, pas seulement grammaire) : remplacements « instead / would » (CR 614), copies (CR 707), animation et couches (CR 613), restrictions et exigences de combat (CR 508–509), modification de coûts (CR 601.2f), prévention (CR 615), durées liées et différées (CR 603.7, 610.3), compteurs | ≈3 000 | primitives moteur ; C1 |
| C10 | **Mots-clés** : balayage complet CR 702, chaque mot-clé en expansion typée vers des primitives ordinaires, rappel validé | ≈700 | C9 pour ceux qui ont besoin d'un sous-système |
| C11 | **Structures de cartes** : exécution des dispositions (Adventure, Omen, MDFC, split, flip, meld, Room, prototype) | ≈200 | — |
| C12 | **Palier C** : produits spéciaux | ≈600 | C11 |

### Jalons

1. **M1** — CI verte, #26 fusionnée, quarantaine documentée (`coverage-targets`).
2. **M2** — parité ligue : ≥ 1 474 cartes de la ligue supportées et exécutables ;
   le site peut passer au nouveau moteur (PR #369 sur le site, audit Oracle relancé).
3. **M3** — 50 % du corpus (≈17 900 cartes) : C1 à C4 en grande partie.
4. **M4** — 80 % du corpus : C5 à C10.
5. **M5** — 100 % des paliers A et B ; la quarantaine est vide.

## Méthode d'exécution (boucle par vague)

1. Sonder `main` (ligue + corpus), produire le classement avec
   `scripts/rank-oracle-blockers.py`, choisir les thèmes du chantier courant qui
   débloquent le plus de cartes et ne se chevauchent pas.
2. Un agent par thème, dans une copie isolée `.tmp/stage-tN` avec sa propre
   cible Cargo (`.tmp/targets/stage-tN`, `CARGO_BUILD_JOBS=4`), un fichier de
   progression repris après tout redémarrage. Deux ou trois agents au plus en
   même temps (mémoire et disque).
3. Preuves par thème : carte réelle, combinaison inédite, quasi-raté qui doit
   échouer, test d'exécution moteur ; suites analyseur et moteur sans nouvel
   échec ; audit des dix derniers sets sans régression ; sondes ligue et corpus
   sans carte perdue.
4. Intégration : branche par thème, une PR par thème avec titre, description,
   références CR, couverture avant / après et risques.
5. Chaque test sorti de la quarantaine par un thème est réactivé dans la même PR.

## Démarrage (vague en cours)

- C0 : correctif CI de #26 en cours (formatage et partitions poussés ; tests
  réauthorés ; quarantaine explicite en préparation).
- C9 : thème « durées, exil lié et déclencheurs différés » en cours.
- C9 : thème « formes de blessures » (chaque créature et chaque joueur,
  blessures réparties, morsure, fight) en cours.
- Prochains thèmes, dans l'ordre : C1 cadres de déclencheurs non revendiqués
  (`whenever you cast/draw/cycle/discard`, `at the beginning of …`) ; C2
  catalogue des actions-mots-clés ; C1 cadres statiques et de remplacement ;
  C9 animation et couches ; C9 restrictions de combat ; C4 « this way ».
