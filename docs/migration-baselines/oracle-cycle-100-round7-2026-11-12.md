# Cycle 7 — destruction non ciblante de collections

Statut : **certifié**.

Ce cycle généralise la lecture de `Destroy all ...` et `Destroy each ...` en
une action ordinaire d'`EffectSequence`. Le parseur produit l'opération
existante `destroyPermanent(eachPermanent(...))`; aucune opération
`destroyAll`, aucun nom de carte et aucune phrase Oracle complète n'ont été
ajoutés.

La première empreinte construite, `72FA7786C2D0C74D532BBD733E03A4E901D307458DB6801CD2A3F9547B963422`,
a servi à découvrir deux défauts de généralisation avant promotion :

- un qualificatif numérique final d'une liste de types était appliqué à la
  dernière branche seulement;
- `Vicious Rivalry` utilisait `xValue` sans `{X}` dans le coût de mana de sa
  face et sans déclaration locale.

Le premier audit 72FA reste une mesure diagnostique, pas la baseline finale.
Le snapshot certifié est
`047577BD11A655EF873ED0B6AE60710D14D56638D291D7BD65CFBFBDC2DB57FB`.
Le build combiné, les quatre audits, les contrats générés et la vérification
indépendante root ont tous été rejoués contre cette empreinte.

## Architecture

Le chemin Oracle est :

1. le parent reconnaît le verbe `destroy` et le quantificateur `all|each`;
2. le child partagé construit une requête d'objets permanents;
3. les types coordonnés forment une union de taille non bornée;
4. un qualificatif numérique final commun devient
   `AND(OR(types...), comparison)` seulement si aucune branche antérieure ne
   possède déjà son propre qualificatif;
5. un contrôleur final commun reste extérieur au prédicat;
6. `EffectSequence` conserve les actions suivantes et leurs déclarations;
7. le lowering émet `destroyPermanent(eachPermanent(...))`.

Les qualificatifs de branches antérieures restent locaux. Une forme comme
`artifacts with mana value 1 and creatures with mana value 3` demeure donc
une union de deux intersections. Un contrôleur placé avant un connecteur ne
peut pas fuiter vers la branche suivante : le parent rejette le programme
incomplètement consommé. Les quantificateurs non implémentés, par exemple
`every`, restent explicitement non couverts.

Le gate de contexte `X`, séparé du sélecteur, accepte une référence
`decisionResult(xValue)` seulement si la règle la déclare ou si la face du sort
porte `{X}` dans son coût. Il conserve Gaze of Granite, Forced March, Meltdown
et Punishment, et refuse la capacité isolée de Vicious Rivalry avec un
diagnostic `card-context-binding` structuré.

## Moteur

Le moteur réutilise la primitive de destruction existante. Le nouveau module
de batch prépare les protections, remplacements, LKI et sources de triggers
avant la première mutation, puis applique le lot. Cela corrige les deux
anciens scénarios rouges : le lord donnant l'indestructible et le remplacement
de cimetière retiré dans le même lot.

Les événements distinguent toujours destruction et décès : un remplacement
vers l'exil peut publier `permanentDestroyed`, mais pas `permanentDied`.

## Preuves verticales

- Parser final 0475 : 125/125 cas, 28 fixtures, build 14,565 s, exécution
  0,666 s. Le premier checkpoint 72FA avait passé 112/112 cas avant les deux
  corrections de revue.
- Runtime final du moteur : 360/360 fixtures, 2 244 assertions, 1,393 s. Il
  inclut Ruinous Ultimatum, Supreme Verdict, les deux anciens gaps, Meltdown
  avec `X=2` et Punishment avec le prédicat groupé.
- Partition unitaire `destruction_batch` : 4/4 tests, compilation 25,941 s,
  exécution 0,871 s.
- Contrats versionnés : 216/216 verts après une liaison de 2,420 s.
- Contrats générés du snapshot final : 80/80 capacités en 0,050 s et 35/35
  cartes complètes en 0,022 s.
- API NativeOnly : 5/5 tests, compilation 2,699 s et exécution 1,613 s.
- Architecture : zéro violation; les quatre avertissements sont les composites
  historiques déjà suivis, aucun n'est produit par ce cycle.

Les règles exactes de Ruinous Ultimatum et Supreme Verdict ont été comparées
entre parseur, contrat et runtime. Punishment dispose d'une preuve runtime de
11 assertions pour `AND(OR(Artifact, Creature, Enchantment), manaValue==X)`.

## Couverture finale 0475

La comparaison est stricte sur toutes les capacités déjà natives, y compris
celles de cartes mixtes.

| Corpus | Capacités antérieures préservées | Gains diagnostiques | Cartes complètes diagnostiques | Régressions |
|---|---:|---:|---:|---:|
| Lot round 6, 100 cartes | 29/29 | 3 | 3 | 0 |
| File rotative, 450 cartes | 247/247 | 3 | 3 | 0 |
| Sept ensembles, 2 069 cartes | 1 126/1 126 | 2 | 2 | 0 |
| Global local, 39 288 cartes | 14 411/14 412 | 80 | 35 | 1 correction manifeste |

Sur les 80 capacités globales, 53 et 32 cartes complètes sont attribuables à la
destruction de collections; 27 capacités et trois cartes complètes proviennent
du chantier parallèle `source-entry-state`. La population des 32 cartes
complètes de collection contient 32 cartes ordinaires : 31 au layout `normal` et
une carte modale double face, Ondu Inversion // Ondu Skyruins. Selon la
taxonomie stable, un layout `modal_dfc` reste une carte ordinaire; les variantes
réservent les objets planar/scheme/vanguard. Cette population ne contient donc
ni variante de jeu, ni carte auxiliaire ou de memorabilia.

L'unique capacité auparavant native retirée est `Draw X cards.` de Necrologia :
le gate contextuel a prouvé qu'elle ne possède ni déclaration `xValue`, ni
`{X}` dans le coût de sa face. Vicious Rivalry, apparue seulement dans le
checkpoint diagnostique 72FA, est également rejetée; Punishment conserve sa
capacité native avec l'IR corrigée. Aucune autre capacité native, y compris sur
les cartes mixtes, n'a changé.

La matrice des huit comparaisons 0F5/72FA est
`.tmp/collection-destroy-comparisons-047577BD-final.json`; elle ne contient
aucune différence inattendue.

## Temps mesurés

Les événements ci-dessous suivent l'ordre de leurs exécutions enregistrées.
Les fenêtres d'implémentation incluent de la revue, de la coordination et de
l'attente; elles ne mesurent pas du codage actif et chevauchent les travaux
parallèles. L'enregistrement d'un handoff ne mesure pas son analyse préalable.

| Étape chronologique | Temps | Description |
|---|---:|---|
| Audit initial du moteur | 311,881 s | Identifier les défauts de simultanéité et préparer le plan |
| Enregistrement du handoff parser | 0,013 s | Copier le résultat du préaudit; durée réelle de l'analyse non isolée |
| Fenêtre d'implémentation moteur | 945,566 s | Fenêtre suivie, avec coordination et attente; non additive aux étapes chevauchées |
| Compilation parser 72FA | 39,097 s | Premier checkpoint de diagnostic |
| Exécution parser 72FA | 0,930 s | 112/112 cas |
| Compilation runner moteur | 50,457 s | Une compilation après le changement de destruction |
| Exécution moteur initiale | 1,934 s | 350/350 scénarios avant ajout des témoins de revue |
| Compilation unités moteur | 25,941 s | Partition de quatre tests, sans parser |
| Exécution unités moteur | 0,871 s | 4/4 tests |
| Liaison runner contrat | 2,420 s | Réutilise le moteur déjà compilé |
| Liaison analyseur 72FA | 1,892 s | Rustc direct, sans Cargo |
| Boucle de correction : compilation parser 90A6 | 25,221 s | Scope des listes, binding X et source nommée |
| Boucle de correction : exécution 90A6 | 0,752 s | 121 cas verts, une fixture JSON invalide; correction de données sans Cargo |
| Exécution moteur finale | 1,393 s | 360/360 scénarios, 2 244 assertions; parser modifié indépendamment |
| Boucle de correction : compilation parser 0475 | 14,565 s | Garde de consommation complète contre collision multi-phrase |
| Exécution parser finale | 0,666 s | 125/125 cas et aucune fixture invalide |
| Compilation API NativeOnly | 2,699 s | Cinq tests de politique/composition |
| Exécution API NativeOnly | 1,613 s | 5/5 tests |
| Audit final du lot 100 | 0,169 s | Temps mur, dont 0,125 s de parsing; 204 capacités |
| Audit final de la file 450 | 0,623 s | Temps mur; 837 capacités |
| Audit final des sept ensembles | 2,668 s | Temps mur, dont 2,219 s de parsing; 4 300 capacités |
| Audit final global | 29,710 s | Temps mur, dont 28,696 s de parsing; 67 420 capacités |
| Contrats des 80 capacités | 0,050 s | Générés depuis le snapshot final |
| Contrats des 35 cartes | 0,022 s | Toutes les règles de chaque carte |

Les sept commandes Cargo de ce tableau totalisent **160,400 s de compilation
et liaison**. Ce total exclut les petites liaisons rustc d'analyseurs. Il ne
représente pas le temps de développement : l'analyse, les corrections et la
revue de sémantique dominent cette fenêtre. Les boucles JSON n'ont pas nécessité
de compilation. Les étapes parser 90A6 et 0475 n'ont pas reconstruit le moteur.

Trackers :

- `.tmp/card-migration-timings/round6-ruinous-ultimatum-mass-destruction.json`;
- `.tmp/card-migration-timings/round6-supreme-verdict-mass-destruction.json`.

Le compagnon structuré est
`docs/migration-baselines/oracle-cycle-100-round7-2026-11-12.json`.
