# Couverture des dix derniers sets - checkpoint 8C34384E

Audit du 12 novembre 2026, snapshot local Scryfall du 9 novembre. Mode **NativeOnly**, sans aucun recours au parseur historique. Les variantes d’impression sont dédupliquées par `oracle_id` dans chaque set; toutes les faces sont évaluées.

| Set | Sortie | Cartes | Entières | Partielles | Sans capacité native |
|---|---|---:|---:|---:|---:|
| Mystery Booster Commander Edition (MBC) | 2026-10-19 | 80 | 8 | 15 | 57 |
| Reality Fracture (FRA) | 2026-10-02 | 285 | 35 | 109 | 141 |
| The Hobbit (HOB) | 2026-08-14 | 193 | 11 | 81 | 101 |
| Marvel Super Heroes (MSH) | 2026-06-26 | 281 | 16 | 141 | 124 |
| Secrets of Strixhaven (SOS) | 2026-04-24 | 271 | 32 | 99 | 140 |
| Teenage Mutant Ninja Turtles (TMT) | 2026-03-06 | 195 | 14 | 79 | 102 |
| Lorwyn Eclipsed (ECL) | 2026-01-23 | 273 | 20 | 80 | 173 |
| Avatar: The Last Airbender (TLA) | 2025-11-21 | 286 | 30 | 94 | 162 |
| Marvel's Spider-Man (SPM) | 2025-09-26 | 193 | 13 | 86 | 94 |
| Edge of Eternities (EOE) | 2025-08-01 | 266 | 20 | 81 | 165 |

**Entière** exige un statut carte canonique et toutes ses capacités canoniques/natives. Trois cartes sans capacité sont incluses (2 FRA, 1 HOB); aucun échec de découpage n’est traité comme une carte sans capacité.

Les lignes totalisent **2 323 occurrences par set**, dont **199 entières**. Après déduplication entre sets : **156 / 2 279 cartes entières (6,85 %)**, **864 partielles**, **1 259 sans capacité native canonique**. Capacités : **1 260 / 4 652 natives (27,09 %)**.

Depuis le premier audit B13 : **+2 cartes entières et +9 capacités natives** sur ces dix sets. Les gains globaux Damn/Pongify n’ajoutent pas de cartes à ces dix sets.

La sélection retient les types Scryfall expansion/core/masters/draft_innovation papier déjà sortis. Mystery Booster Commander Edition est inclus car classé draft_innovation. Les suppléments Commander, promos, jetons et sets numériques sont exclus. Final Fantasy est le onzième selon ce critère.

Les nombres mesurent la reconnaissance native. Ils ne prouvent pas individuellement le comportement dynamique de chaque carte. Le checkpoint possède 194/194 cas du parseur, 228/228 contrats permanents, 216/216 contrats générés, 401/401 scénarios moteur et les preuves ciblées des nouveaux automates.

L’analyseur exporte aussi le JSON : son temps inclut donc parsing et sérialisation. Le lancement et l’agrégation sont mesurés séparément. Voir les temps par set dans le résumé JSON et le rapport d’audit détaillé.

Analyseur SHA-256 : `0243BBF72F6D683125D1E18547139DFDF678FE657BF830CCE81E8BC8C673E9DA`. Rapport détaillé SHA-256 : `F8677CD6A12A42B5806EE00EDA8C53D21024992754F7F040E4E823D4F74A1F31`.

Résumé : `last-10-sets-native-coverage-8C34384E-2026-11-12.json`. Vérification indépendante : `.tmp/recent-sets-native-8C34384E/root-independent-verification.json`.
