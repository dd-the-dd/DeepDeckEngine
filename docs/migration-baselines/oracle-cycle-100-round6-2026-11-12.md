# Cycle 6 — jeu de terrains additionnels

Le concept `additionalLandPlay` est maintenant couvert de bout en bout par un
sous-automate lexical partagé, un modificateur joueur canonique et le
comportement déjà générique du moteur. Le chemin ne contient ni regex, ni nom
de carte, ni phrase Oracle complète codée en dur.

Le snapshot certifié produit par le build combiné et utilisé par les audits est
`0F5B240B45AFDBFA0D8FCB69412ACA25EBC52595528E24AEE2686269FF4532BF`.

## Grammaire et IR

Le sous-automate reconnaît la structure :

> PLAYER may play QUANTITY additional land(s) on each of PLAYER's turns.

Il délègue `PLAYER` au reconnaisseur partagé de références joueur et
`QUANTITY` à `NumericExpression`. Le parent statique impose actuellement
`controllerOf(self)` et la durée persistante « on each of your turns ». Il
refuse explicitement une durée temporaire, un autre joueur, un accord
singulier/pluriel invalide, une variable non résolue, zéro, une valeur négative
ou une valeur supérieure à `i32::MAX`.

L'IR produite est une `staticAbility`, active tant que la source est sur le
champ de bataille, contenant un modificateur indépendant :

```json
{
  "kind": "additionalLandPlay",
  "player": { "kind": "controllerOf", "object": { "kind": "self" } },
  "count": { "kind": "integer", "value": 1 }
}
```

Azusa réutilise exactement la même structure avec `count.value = 2`; aucune
branche dédiée à la carte n'est ajoutée.

## Preuves verticales

- Parseur NativeOnly : 103/103 cas verts. Les huit cas ciblés couvrent
  Exploration, Azusa, une variante spéciale, la durée, le joueur, l'accord du
  nom, une variable non déclarée et le dépassement de `i32::MAX`.
- Contrats du dépôt : 174/174 verts, dont les contrats exécutables complets
  d'Exploration et d'Azusa.
- Runtime ciblé : 5/5 scénarios, 27 assertions, zéro échec en 0,098 s. Les
  scénarios prouvent le quota normal de un terrain, Exploration à deux terrains,
  Azusa à trois, l'absence de fuite vers l'adversaire et la saturation sans
  débordement.
- Runtime global fraîchement lié : 347/347 scénarios et 2 145 assertions verts
  en 1,414 s.
- Contrats générés du snapshot combiné décès + terrains : 56/56 capacités
  exécutables. Cette population est distincte des 174 contrats versionnés.
- Égalité stricte : pour Exploration et Azusa, l'IR du parseur 0F5B est égale
  à l'IR du contrat, à la règle racine du scénario runtime et à la règle
  réellement attachée à la source sur le champ de bataille. La preuve est dans
  `.tmp/additional-land-play-parser-contract-runtime-equality.json`.

Le moteur calcule encore le bénéficiaire du modificateur depuis le contrôleur
de la source plutôt que de lire librement `modifier.player`. Ce contrat est
cohérent avec le parseur actuel, qui refuse toute référence autre que
`controllerOf(self)`. Élargir les joueurs concernés exigera donc une tranche
runtime séparée, et non un assouplissement silencieux du parseur.

## Couverture mesurée

La comparaison immuable utilise 9DFF comme baseline et 0F5B comme résultat.
Toutes les capacités auparavant natives sont comparées, même sur les cartes
encore mixtes ou non couvertes.

| Corpus | Capacités antérieures préservées | Gains `additionalLandPlay` | Cartes complètes attribuables | Régressions |
|---|---:|---:|---:|---:|
| Lot round 5, 100 cartes | 26/26 | 2 | 1 | 0 |
| File rotative, 450 cartes | 242/242 | 3 | 1 | 0 |
| Sept ensembles, 2 069 cartes | 1 121/1 121 | 4 | 0 | 0 |
| Global local, 39 288 cartes | 14 369/14 369 | 22 | 4 | 0 |

Dans le lot, Exploration devient entièrement native; Icetill Explorer gagne
la capacité statique mais conserve d'autres bloqueurs. Sur le catalogue global,
les quatre cartes entièrement natives sont :

- Exploration et Azusa, Lost but Seeking, deux cartes ordinaires;
- Titania, une carte Vanguard (`layout=vanguard`, `type_line=Vanguard`);
- The Explorer, une carte Hero spéciale (`layout=normal`,
  `set_type=memorabilia`), et non une deuxième Vanguard.

Le snapshot combiné contient au total 43 nouvelles capacités natives et 13
nouvelles cartes entièrement natives : 22 capacités et quatre cartes viennent
de `additionalLandPlay`; les autres gains appartiennent au chantier parallèle
`permanentDied` et aux séquences déjà documentées. La revue racine indépendante
confirme 14 369 capacités préservées et zéro régression dans
`.tmp/death-and-land-global-root-verification.json`.

## Temps mesurés

| Étape chronologique | Temps | Description |
|---|---:|---|
| Analyse | 80,550 s | Lexique `turns`, IR existante, runtime et frontières génériques |
| Fenêtre d'implémentation | 321,672 s | FSM, lowering, fixtures et coordination; inclut des attentes |
| Formatage + validation JSON | 0,389 s | Aucun build |
| Build parseur combiné | 30,523 s | Snapshot 0F5B |
| Exécution parseur | 0,645 s | 103/103 cas |
| Build runtime/contrats | 40,103 s | Runners partagés liés une fois |
| Runtime ciblé | 0,098 s | 5 fixtures, 27 assertions |
| Liaison analyseur | 1,107 s | `rustc` direct contre le rlib frais, sans Cargo |
| Audit lot 100 | 0,502 s | 201 capacités |
| Audit file 450 | 0,545 s | 837 capacités |
| Audit sept ensembles | 2,545 s | 4 300 capacités |
| Audit global | 28,769 s | 67 420 capacités |

Le tracker complet est
`.tmp/card-migration-timings/round5-46-exploration-additional-land-play.json`.
Il compte zéro boucle corrective; ses grandes fenêtres `focused-tests` et
`runtime-proof` incluent surtout l'attente de la lane Cargo unique.

## Lot round 6

Le script officiel `scripts/advance-oracle-migration-batch.ps1` a conservé les
97 cartes encore non couvertes et remplacé uniquement les trois cartes promues
du lot précédent : Exploration, Pitiless Plunderer et Blood Artist.

Les trois recrues sont :

| Provenance | Rang global | Carte | Oracle ID |
|---|---:|---|---|
| league | 159 | Aether Vial | `fc148e1e-dff0-448e-9f16-625341754356` |
| league | 162 | Crop Rotation | `28b46183-c62f-47b1-9fee-3ba148202cab` |
| priority | 176 | Timeless Lotus | `fec77e0a-433e-4d32-9260-e26a37f6ad23` |

Les quotas restent 34 cartes récentes, 33 prioritaires et 33 de ligue. Azusa
n'était pas dans le lot round 5 et n'est donc pas retirée ou comptée une
seconde fois.

Artefacts du lot :

- `.tmp/rotating-next-100-round6.json`, SHA-256
  `B6BA7114D40FD564DD71C4E2538597E4795B4AC94A8289D54B6E4958AB7CB05F`;
- `.tmp/rotating-next-100-round6-input.json`, SHA-256
  `D6D4C680A1BC0218CDBB29A53FB5A5ED350BC676690595F1E1132DFF8CE9B355`.

Les hashes de la queue, de l'audit 0F5B, de l'entrée Oracle et du lot précédent
sont enregistrés directement dans `baseline_hashes` du document round 6.

Le compagnon structuré de ce rapport est
`docs/migration-baselines/oracle-cycle-100-round6-2026-11-12.json`.
