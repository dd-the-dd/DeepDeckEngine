# Cycle 9 — parent modal générique piloté par les tokens

Statut : **certifié** sur le checkpoint
`74DE902E2A30F7636738C7BA638FF89A97A894989734E2B05E6031F8F782E27B`.

Ce cycle remplace la feuille regex exacte d’Abrade par un parent modal commun.
Le parent possède seulement `Choose N —`, la cardinalité exacte et les
frontières `•`; chaque mode est une `EffectSequence` ordinaire. Abrade et
Austere Command sont les deux cartes prioritaires certifiées de bout en bout.

## Architecture

Le chemin natif suit ces frontières :

1. le claim structurel exige `Choose`, un nombre littéral positif, le tiret
   cadratin et au moins une puce;
2. les formes `one or both`, `one or more`, conditionnelles ou répétables ne
   sont pas revendiquées par ce parent exact;
3. le parent borne chaque puce et rebascule les indices de `ResolvedNameLink`
   et des échecs de résolution sans modifier les spans absolus;
4. `recognize_effect_sequence` est la seule autorité sur les actions du mode;
5. `lower_local_effect_sequence` produit les décisions et effets primitifs;
6. seuls les IDs créés par un mode sont préfixés `modeN:` et conditionnés par
   `selectionContains(chosenModes, modeN)`;
7. les bindings externes, dont `xValue`, ne sont ni créés, ni préfixés, ni
   conditionnés par le parent;
8. les effets restent dans l’ordre imprimé, indépendamment de l’ordre dans
   lequel le joueur sélectionne les modes.

La route spécialisée primitive existante précède le parent générique. Elle
conserve les modes dont les enfants ne vivent pas encore tous dans
`EffectSequence`, notamment Kozilek’s Command et Boros Charm. Il n’existe
aucun fallback regex en `NativeOnly`. L’ancienne feuille exacte
`parse_choose_one_modal` d’Abrade a été retirée.

Le lowering commun d’`EffectSequence` sait désormais déclarer et exécuter un
`dealDamage` ciblé et un changement de zone ciblé comme deux actions
ordinaires. Plusieurs actions de même domaine reçoivent des IDs distincts.

## Contrat IR

La décision modale est :

```json
{
  "id": "chosenModes",
  "kind": "chooseModes",
  "minimum": 2,
  "maximum": { "kind": "integer", "value": 2 },
  "options": ["mode1", "mode2", "mode3", "mode4"],
  "allowRepeated": false
}
```

Abrade ajoute `mode1:targetPermanent` pour la créature et
`mode2:targetPermanent` pour l’artefact. Austere Command n’ajoute aucune cible
et abaisse ses quatre modes vers `destroyPermanent(eachPermanent(...))`.

Le sibling `{X}{R}` conserve `decisionResult(xValue)` dans le montant de
dommages sans produire de décision `xValue`. Le gate contextuel accepte ce
binding grâce au `{X}` de la même face et refuse la variante `{R}` dans
`binding.x-value.source`.

## Preuves

- Parser ciblé : 6/6 cas, compilation 31,257 s, exécution 0,566 s.
- Régression complète des fixtures parser : verte en 0,288 s sans rebuild.
- Contrats versionnés : 218/218 verts.
- Contrats générés : 29/29 capacités gagnées et 21/21 cartes complètes,
  soit 50/50 en 0,133 s.
- Runtime exact : quatre vrais `castSpell` puis résolution — deux modes
  d’Abrade, Austere Command et le sibling Darigaaz’s Charm — 29 assertions en
  0,068 s.
- Gate runtime final : 366/366 scénarios verts en 1,468 s.
- Architecture : zéro violation; quatre avertissements historiques.
- Égalité IR : parseur, contrat et runtime ont les mêmes règles complètes pour
  Abrade et Austere Command; Darigaaz’s Charm possède aussi un témoin runtime
  de cible de carte correctement scopée.

## Couverture 74DE

La comparaison protège chaque capacité déjà native, y compris celles des
cartes mixtes.

| Corpus | Capacités natives préservées | Gains natifs | Cartes complètes gagnées | Régressions |
|---|---:|---:|---:|---:|
| Lot round 7, 100 cartes | 27/27 | 2 | 2 | 0 |
| File rotative, 450 cartes | 250/250 | 2 | 2 | 0 |
| Sept ensembles, 2 069 cartes | 1 128/1 128 | 1 | 0 | 0 |
| Global local, 39 288 cartes | 14 491/14 491 | 29 | 21 | 0 |

Les 21 cartes complètes gagnées sont toutes des cartes ordinaires au layout
`normal`. Abrade et Austere Command ont une preuve comportementale exacte;
les 19 autres gains disposent du parseur natif et d’un contrat exécutable, mais
pas tous d’un scénario runtime individuel. Necrologia reste explicitement
refusée par le gate contextuel `xValue`.

La vérification indépendante root est
`.tmp/modal-global-root-verification-74DE902E.json`. Les règles exactes sont
comparées dans `.tmp/modal-parser-contract-runtime-equality-74DE902E.json`.

## Temps mesurés

| Étape chronologique | Temps | Description |
|---|---:|---|
| Analyse | 170,935 s | Contrat chooseModes, frontières des puces, rebasing et lowering partagé |
| Implémentation | 492,297 s | Parent modal, lowering commun, fixtures et retrait legacy; fenêtre avec coordination |
| Boucle corrective | 1 boucle | Import manquant et suppression du faux producteur `xValue` avant le premier build |
| Compilation parser | 31,257 s | Build unique du runner, empreinte 74DE |
| Exécution parser ciblée | 0,566 s | 6/6 cas |
| Régression parser | 0,288 s | Réutilisation du runner frais |
| Liaison analyseur | 1,106 s | Rustc direct contre le rlib frais, sans Cargo |
| Audit lot 100 | 0,630 s | Temps mur |
| Audit file 450 | 0,553 s | Temps mur |
| Audit sept ensembles | 2,516 s | Temps mur |
| Audit global | 30,675 s | Temps mur; parsing interne 29,672 s |
| Comparaisons strictes | 5,519 s | Quatre corpus contre 0475 |
| Contrats générés | 0,133 s | 50/50 |
| Runtime exact | 0,068 s | 4/4, 29 assertions |
| Runtime final | 1,468 s | 366/366 |
| Architecture | 8,734 s | Zéro violation |

Trackers :

- `.tmp/card-migration-timings/round7-abrade-modal-parent.json`;
- `.tmp/card-migration-timings/round7-austere-command-modal-parent.json`.

Le compagnon structuré est
`docs/migration-baselines/oracle-cycle-100-round9-2026-11-12.json`.
