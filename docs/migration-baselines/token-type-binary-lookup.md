# Classification lexicale avec dictionnaire binaire partagé

`token_type` délègue à `src/oracle/syntax/token_vocabulary.rs`. Les définitions
restent regroupées et ordonnées : 74 groupes de mots et 9 reconnaisseurs de
formes ouvertes. Un `OnceLock` construit, au premier usage, 306 entrées triées
et les conserve pour toute la vie du processus, entre cartes et entre threads.

Le constructeur rejoue les définitions complètes pour chaque clé finie : la
première règle gagnante, y compris un pattern ouvert, détermine la classe mise
en cache. Cela conserve les priorités de `at`, `a/an`, `player's`, `nonbasic`
et des définitions répétées. Le constructeur ne rappelle pas le cache et ne
peut pas rentrer dans son propre `OnceLock`.

Chaque lookup normalise la casse ASCII puis utilise `binary_search_by_key`.
En cas d'absence, seuls les patterns ouverts sont évalués, dans leur ordre
initial : symboles, ordinaux, possessifs de types, négations, caractéristiques,
sous-types Army, valeurs P/T et quantités. Les noms inconnus ne sont pas ajoutés
au dictionnaire. Les passes contextuelles après la classification restent
inchangées : une classe lexicale n'est pas une interprétation de la phrase.

## Mesure

Benchmark isolé de classification, ancienne et nouvelle fonctions compilées
ensemble avec `rustc -C opt-level=3`. Tokens obtenus avec le scanner existant
sur les textes du même corpus local immuable de 39 288 objets. Les entrées
sont préparées avant le chronomètre. Cinq passages complets alternent l'ordre
des deux implémentations ; `black_box` empêche l'élimination du travail.

| Mesure | Résultat |
|---|---:|
| Tokens réels classifiés par passage | 1 245 358 |
| Variantes de contrôle supplémentaires | 936 |
| Ancienne fonction, médiane | 307,903 ms |
| Dictionnaire binaire, médiane | 200,734 ms |
| Réduction du temps de classification | 34,8 % |
| Ratio des débits de classification | 1,534 |
| Construction du dictionnaire, vocabulaires de support déjà chargés | 189,5 µs |

Tous les résultats `(category, token_type)` des tokens réels et variantes sont
identiques. Ce benchmark mesure uniquement la classification de base ; il
ne mesure pas un gain de 34,8 % sur le parser complet, la simulation ou la
compilation. Les coûts de chargement initial des autres vocabulaires ne sont
pas inclus dans les 189,5 µs.

## Validation

- 3 tests dédiés : priorités lexicales, formes ouvertes/near-misses et partage
  de la même allocation entre lookups et threads.
- 36 tests existants du tokenizer passent.
- 397 fixtures parser passent dans les deux modes, zéro fixture invalide.
- Garde d'architecture : zéro violation, zéro avertissement.
- Comparaison des tokens Rust : seuls `token_type` et son petit helper de
  types d'objet changent dans le tokenizer ; les autres fonctions sont
  identiques hors formatage. Les listes de types n'ont qu'une définition.
- Aucun changement du moteur ou de l'IR ; index Git inchangé.

Les timings de compilation et d'exécution des tests sont séparés dans
`.tmp/token-type-lookup/`. Le benchmark, ses cinq échantillons, la copie de
référence précédente et les contrôles de source sont conservés dans ce dossier.
L'ancienne chaîne de classification n'est pas conservée dans le code de
production ni recompilée à chaque lookup.
