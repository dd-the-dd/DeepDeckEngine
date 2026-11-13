# Cycle 10 — augmentation générique du coût de lancement des sorts

Statut : **certifié** sur le checkpoint
`F92B198017939234D084332B50CDFB8A2CB5D611057A786EE036EE6C839B0612`.

Ce cycle couvre de bout en bout **Thalia, Guardian of Thraben** et généralise
la phrase statique « les sorts satisfaisant un prédicat coûtent `{N}` de plus
à lancer ». Le parseur produit un unique modificateur primitif
`additionalCastingCost`; le moteur compose ensuite coût de base ou alternatif,
augmentations, valeur de `X`, réductions, plancher et substitutions de paiement.

## Architecture

Le chemin natif respecte les frontières suivantes :

1. l'enveloppe token-only réclame uniquement une phrase complète
   `OBJECT-QUERY cost MANA more to cast.`;
2. le sous-automate `ObjectQuery` possède les types, couleurs, négations,
   unions, intersections et la relation du lanceur;
3. le sous-automate mana possède le coût additionnel fixe;
4. `{X}` est refusé dans cette famille tant qu'il n'est pas lié par un contrat
   de coût additionnel distinct;
5. la source doit être une face de permanent, car la règle est active sur le
   champ de bataille;
6. le lowering produit `additionalCastingCost` sans opération combinée ni cas
   nommé;
7. la famille native plus précise « chosen name » garde la précédence sur la
   famille générique. Ainsi, `with the chosen name` n'est jamais interprété
   comme un qualificatif de mot-clé.

La correction moteur partagée applique les étapes de coût dans l'ordre : coût
de base ou alternatif sélectionné, augmentations, résolution de `X`, réductions,
plancher, puis substitutions de paiement telles que delve. Les coûts
additionnels ne sont donc plus réduits dans un ordre distinct du coût de base.

## Contrat IR

La règle de Thalia est composée de `firstStrike` et de cette primitive statique :

```json
{
  "kind": "staticAbility",
  "source": { "kind": "self" },
  "activeWhile": {
    "kind": "inZone",
    "object": { "kind": "self" },
    "zone": { "kind": "battlefield" }
  },
  "modifiers": [{
    "kind": "additionalCastingCost",
    "players": { "kind": "eachPlayer" },
    "where": {
      "kind": "not",
      "operand": { "kind": "cardTypeContains", "value": "Creature" }
    },
    "cost": { "kind": "payMana", "manaCost": "{1}" }
  }]
}
```

Le parseur, le contrat versionné et les règles embarquées dans les deux
scénarios runtime de Thalia sont structurellement identiques.

## Preuves exactes

### Thalia, Guardian of Thraben

- parser : carte canonique, deux capacités natives;
- contrat versionné : règle complète exécutable;
- runtime disponibilité : le sort non-créature du contrôleur et celui de
  l'adversaire sont taxés, le sort de créature ne l'est pas;
- runtime paiement : le coût `{1}` et la taxe `{1}` sont payés ensemble;
- total : deux scénarios exacts, 12 assertions vertes.

### Disruptor Flute

Le premier audit `3F81` a révélé une collision de précédence : la famille
générique interceptait `Spells with the chosen name cost {3} more to cast.`.
Le dispatch final donne d'abord la phrase à la famille native `chosen name`.
Sous `F92B`, la carte et ses quatre capacités sont **identiques octet pour
octet** au checkpoint `74DE`; le prédicat reste
`nameEquals(storedDecision(chosenCardName))`.

## Généralisation mesurée

La vérification indépendante root sur 39 288 objets préserve exactement
14 520 capacités natives antérieures et ajoute 13 capacités natives, sans
régression.

Onze cartes deviennent entièrement natives :

- neuf formes simples : Chill, Feroz's Ban, Glowrider, Lodestone Golem,
  Sphere of Resistance, Squeeze, Thalia, Thorn of Amethyst et Vryn Wingmare;
- deux unions : High Seas et Irini Sengir.

Deux capacités supplémentaires deviennent natives sans rendre leur carte
entièrement native :

- `White spells cost {3} more to cast.` sur Gloom;
- `Noncreature spells cost {1} more to cast.` sur Charitable Levy.

Ces gains partiels restent comptés comme capacités, jamais comme cartes
complètes. Les 13 capacités gagnées et les 11 règles complètes de cartes ont
24 contrats générés, tous exécutables.

| Corpus | Cartes | Capacités natives préservées | Gains natifs | Cartes complètes gagnées | Régressions |
|---|---:|---:|---:|---:|---:|
| Lot round 8 | 100 | 27 | 1 | 1 | 0 |
| File rotative | 450 | 252 | 1 | 1 | 0 |
| Sept ensembles locaux | 2 069 | 1 129 | 1 | 0 | 0 |
| Global local | 39 288 | 14 520 | 13 | 11 | 0 |

## Gates

- Fixtures parser : 140/140 vertes, compilation 13,650 s, exécution 0,697 s.
- Contrats du dépôt : 219/219 verts.
- Contrats générés : 13 capacités + 11 cartes, 24/24 verts en 0,022 s.
- Tests moteur `casting_cost` : 12/12 verts; compilation 38,552 s,
  exécution du wrapper 1,538 s.
- Régression runtime : 369/369 scénarios, 2 299 assertions, exécution 1,439 s.
- Audit global : 30,347 s de parsing, 31,345 s mur.
- Liaison directe de l'analyseur contre le `rlib` frais : 1,116 s, sans Cargo.

Les scénarios runtime individuels certifient Thalia et la composition générique
taxe/réduction. Les dix autres cartes complètes gagnées ont un parseur natif et
un contrat exécutable, mais pas chacune un scénario runtime nommé.

## Refill du lot

Le script officiel `advance-oracle-migration-batch.ps1` a retiré Thalia, gardé
les 99 autres cartes non natives et recruté **Dark Depths** depuis la source
`league`. Le prochain lot contient exactement 100 cartes :

- sélection : `.tmp/rotating-next-100-round9.json`, SHA-256
  `3B15331E726A40CCF6DE9B571EDF3F007AFBB06DD4F485930759EE7DBF8E4F8E`;
- entrée Oracle : `.tmp/rotating-next-100-round9-input.json`, SHA-256
  `3F02AE397D7115DCF0F350D406DF4ED0D576BB112AAC02C9E3DE4B9626818C36`.

## Artefacts

- vérification globale indépendante :
  `.tmp/spell-tax-global-root-verification-F92B1980.json`;
- égalité Thalia parseur/contrat/runtime :
  `.tmp/spell-tax-thalia-parser-contract-runtime-equality-F92B1980.json`;
- restauration Flute : `.tmp/spell-tax-disruptor-flute-restoration-F92B1980.json`;
- contrats générés : `.tmp/spell-tax-contracts-F92B1980` et
  `.tmp/spell-tax-generated-contract-run-F92B1980.json`;
- moteur : `.tmp/casting-total-cost-unit-final-timing.json` et
  `.tmp/casting-total-cost-runtime-final.json`;
- tracker : `.tmp/card-migration-timings/round9-thalia-static-spell-tax.json`.

Le compagnon structuré est
`docs/migration-baselines/oracle-cycle-100-round10-2026-11-12.json`.
