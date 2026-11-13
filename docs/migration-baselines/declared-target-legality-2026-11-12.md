# Légalité partagée des cibles déclarées — 2026-11-12

La résolution et le changement de cible utilisent maintenant la même déclaration
typée `chooseTargets`. Le moteur retrouve l'identifiant exact de la décision, y
compris les espaces de noms modaux comme `mode1:targetPermanent`, puis reconstruit
les candidats avec les décisions engagées, la source vivante ou son LKI et le
joueur de zone résolu dans le contexte de la pile.

Le défaut public était une cible devenue illégale après un changement de
contrôleur : l'effet s'appliquait encore. La fixture
`permanent-entered-target-controller-revalidation.json` est passée de rouge à
verte et vit désormais dans le gate runtime ordinaire, plus dans `known-gaps`.

## Contrat sémantique

- À la résolution, chaque cible est revalidée indépendamment contre son domaine
  courant, conformément à CR 608.2b.
- Les contraintes conjointes (`sameZoneOwner`, `distinctTargets`) ne servent
  qu'au choix initial ou au changement de cible.
- Une condition de disponibilité n'est pas réévaluée pour un slot déjà engagé.
- Les comparaisons à `xValue` utilisent les décisions de pile et les puissance
  et endurance courantes, incluant notamment les marqueurs.
- `excludeSource`, `attachedPermanent`, les zones liées à `chosenTarget` ou
  `ownerOf`, les unions et les sources LKI conservent leur contexte.
- Un identifiant implicite sans déclaration fiable garde le chemin historique;
  le correctif ne fabrique pas une déclaration.

Les 15 nouvelles capacités ciblées du parent `permanentEntered` possèdent toutes
une unique décision de cible avec `minimum = maximum = 1`. Elles passent donc par
la frontière corrigée. Il s'agit d'une propriété vérifiée sur le manifeste
`.tmp/permanent-entered-global-root-verification-FAAC58D9.json`, pas de quinze
preuves runtime individuelles.

## Preuves

| Preuve | Résultat | Temps |
|---|---:|---:|
| Fixture rouge devenue verte | 1/1, 5 assertions | build 50,238 s; exécution 0,598 s |
| Unitaires `target_legality` | 8/8 | compilation 39,760 s; wrapper 1,444 s; harnais 0,01 s |
| Gate runtime final | 380/380, 2 364 assertions | build cache 0,204 s; exécution 1,615 s |
| Contrats du dépôt | 223/223 | build 2,311 s; exécution 1,821 s |

Artefacts :

- `.tmp/target-legality-runtime-build.json`;
- `.tmp/target-legality-unit-timing.json`;
- `.tmp/target-legality-runtime-final.json`;
- `.tmp/target-legality-contract-regression.jsonl`;
- `.tmp/target-legality-contract-timing.json`.

## Portée certifiée et limites

La couverture du parseur reste 119 capacités et 51 objets entièrement reconnus :
50 cartes ordinaires de layout `normal` et le jeton Zabu. Les preuves runtime
verticales nommées couvrent quatre cartes — Tatyova, Benthic Druid, Soul Warden,
Wirewood Savage et Aura Shards — et les siblings génériques; elles ne doivent pas
être confondues avec une certification runtime individuelle des 51 objets.

Trois frontières restent ouvertes :

1. pour une véritable sélection multi-cible, le moteur ne retire pas encore
   chaque slot devenu illégal avant d'exécuter un effet qui lit toute la
   sélection;
2. la génération de candidats de zone reste limitée aux formes de zone et de
   référence joueur déjà prises en charge par `target_candidates`;
3. certains prédicats d'événement utilisent encore les caractéristiques imprimées
   de l'objet plutôt que toutes ses caractéristiques courantes ou LKI.

Ces limites ne remettent pas en cause les 15 gains ciblés actuels, tous mono-cible,
mais empêchent d'étendre cette certification à des sélections ou événements plus
larges sans preuve supplémentaire.
