# Cycle 8 — source courante nommée entrant engagée

Statut : **certifié sur le checkpoint `047577BD`**.

Ce cycle étend l’automate existant de remplacement d’état d’entrée de
`This TYPE enters tapped.` à une référence de source courante déjà résolue par
le linker lexical, par exemple `Timeless Lotus enters tapped.`. Le parseur ne
compare jamais le texte au nom de Timeless Lotus et n’ajoute aucune opération
moteur : il réutilise `wouldEnterBattlefield` et `setEnteringState`.

## Architecture

Le chemin promu est entièrement piloté par les tokens :

1. le classifier reconnaît l’enveloppe `SOURCE enters tapped|prepared .`;
2. `recognize_source_reference` consomme soit `this TYPE`, soit un
   `ResolvedNameLink` `CurrentCard|CurrentFace` de largeur quelconque;
3. l’automate conserve le type explicite de `this TYPE`, mais une référence
   nommée n’invente aucun type à partir de ses mots;
4. le parent vérifie toujours que la face courante possède un type de permanent;
5. le lowering émet le remplacement atomique préexistant.

IR exact :

```json
{
  "kind": "replacementEffect",
  "source": { "kind": "self" },
  "event": {
    "kind": "wouldEnterBattlefield",
    "object": { "kind": "self" }
  },
  "replacement": [{
    "kind": "setEnteringState",
    "object": { "kind": "self" },
    "tapped": true
  }]
}
```

La frontière de promotion exige que le point suivant l’état termine la
capacité. Seule la forme `prepared` peut encore porter son rappel canonique
parenthésé complet. Les formes conditionnelles comme
`Port Town enters tapped unless you reveal ...` et les capacités composées
comme `This land enters tapped. As it enters, ...` ne sont donc pas
interceptées par ce parent inconditionnel; elles restent au parent ou au
fallback qui possède leur continuation. Une face d’instant ou de rituel est
refusée même si le texte artificiel dit `this land`.

## Moteur

Le moteur appliquait déjà `setEnteringState` à la carte mutable avant son
insertion sur le champ de bataille, la publication de la résolution et les
triggers d’arrivée. Aucun code moteur n’a changé.

Deux scénarios exacts Timeless Lotus sont verts sur le runner runtime frais :

- la Lotus lancée depuis la main arrive engagée et sa capacité de mana est
  indisponible;
- une Lotus déjà dégagée sur le champ de bataille s’engage et produit
  `{W}{U}{B}{R}{G}`.

Résultat certifié : **2/2 fixtures, 11 assertions, 0,058 s**, dans
`.tmp/timeless-lotus-runtime-root.jsonl`.

## Preuves parser

Le checkpoint parser final court `047577BD` est vert : **125/125 cas**,
compilation **14,565 s**, exécution **0,666 s**. Il inclut l’exact Timeless
Lotus, un sibling renommé, les near-miss de face non permanente et les gardes
d’anti-collision Port Town/Thriving Isle.

## Couverture globale 047577BD

Le replay global mesure **27 capacités natives gagnées** et **3 cartes devenues
entièrement natives** : Enchanted Prairie
(`60b45aa2-42cb-4eda-bdd6-20128fb0c530`), Winter Soldier, Bucky Barnes
(`874bbeb2-aa15-41e8-afad-122ab9b555fd`) et Timeless Lotus
(`fec77e0a-433e-4d32-9260-e26a37f6ad23`). Le nom avec virgule de Winter
Soldier est celui du snapshot; ce n’est pas une carte à deux faces.
L’estimation de classement 33/4 n’est donc pas utilisée comme résultat.

Pour Timeless Lotus, les deux règles exactes — remplacement d’entrée et capacité
de mana — sont égales entre le parseur, le contrat et les scénarios runtime.
La vérification structurée est dans
`.tmp/collection-bindings-source-entry-global-root-verification.json`.

Enchanted Prairie et Winter Soldier, Bucky Barnes sont des gains mesurés par
l’audit global et les contrats générés. Ce cycle ne leur attribue pas de
nouveau scénario runtime individuel.

## Certification finale

- parser : 125/125 cas sur le binaire `047577BD`;
- contrats générés : 80 capacités gagnées et 35 cartes complètes, tous verts;
- contrats versionnés du dépôt : 216/216 verts;
- API ciblée : 5/5 tests verts;
- architecture : zéro violation;
- runtime exact Timeless Lotus : 2/2 scénarios, 11 assertions;
- égalité stricte des deux règles Timeless Lotus entre parseur, contrat et
  runtime;
- audit global : +27 capacités attribuées au source-entry nommé et +3 cartes
  entièrement natives.

La provenance globale est conservée dans le rapport de vérification racine.

## Temps mesurés

| Étape chronologique | Temps | Description |
|---|---:|---|
| Analyse runtime | 278,188 s | Vérification validation, application avant entrée et publication |
| Implémentation | 320,375 s | FSM, classifier token-only, lowering et fixtures |
| Boucle corrective | 1 boucle | Face permanente toujours vérifiée; anti-collision conditionnelle |
| Boucle corrective | 1 boucle | Point terminal; anti-collision Thriving Isle multi-phrase |
| Compilation parser finale | 14,565 s | Binaire court `047577BD`, 125/125 cas |
| Exécution parser finale | 0,666 s | Exact, sibling et gardes négatifs |
| Runtime exact | 0,058 s | 2 fixtures, 11 assertions; aucun build attribué à cette exécution |

Tracker :
`.tmp/card-migration-timings/round6-timeless-lotus-entry-tapped.json`.

Le compagnon structuré est
`docs/migration-baselines/oracle-cycle-100-round8-2026-11-12.json`.
