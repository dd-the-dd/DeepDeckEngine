# Scry comme action composable : checkpoint 6F673

Lot accepté : **3 797 objets non vides entièrement natifs, 17 628 capacités**.
Le même corpus contient 39 288 objets locaux et 67 417 capacités; les objets
vides restent séparés. Gain : **117 objets complets et 186 capacités**.
Toutes les 17 442 anciennes IR sont strictement identiques.

La preuve individuelle ajoutée est Preordain. Le registre passe de 165 à 166
cartes, sans changement structurel des anciennes entrées. Les autres gains
globaux sont des succès complets de lecture, pas 116 nouvelles simulations
individuelles prétendument exécutées.

## Changement générique

Scry devient un enfant ordinaire de la séquence d'effets. Il réutilise le lecteur
numérique et la procédure de partition de bibliothèque déjà partagée avec
surveil : regarder, choisir, ordonner, déplacer, puis émettre le signal scry.
Le parent fournit un espace de noms distinct pour les quatre liaisons locales
de chaque action. Aucun opcode scry-then-draw n'est ajouté.

L'ancien parent scry atomique rend explicitement les séquences au programme
contextuel, avant tout abaissement de compatibilité. Son IR atomique historique
reste inchangée. Le programme contextuel exige un vrai producteur de X sur
la bonne face; l'ancien comportement atomique Scry X reste une dette distincte.

Un rappel final est validé dans son contexte : introducteur scry ou inspection
de bibliothèque, puis dernière action de partition pertinente. Un rappel Food
n'est pas capturé par un scry antérieur; un surveil plus récent bloque ce
rattachement. Les nombres répétés doivent correspondre et le parent exige la
consommation complète. Les continuations non représentées restent refusées.

## Preuves et coûts chronologiques

| Étape | Compilation / liaison | Exécution | Description |
|---|---:|---:|---|
| Analyse enregistrée | — | 148,998 s | Lecture des frontières existantes; première sélection environ une minute avant le tracker, déclarée séparément |
| Premier harnais d'unités | 35,710 s | 1,570 s | 926/927; un nouveau test supposait à tort le support des virgules seules |
| Test corrigé et préservation du point final | 7,110 s | 1,410 s | 927 passent; la grammaire des virgules seules reste une dette explicite |
| Première lecture DATA | 10,124 s | 1,037 s | 600 cas natifs passent |
| Correction de propriété des rappels après revue | 9,619 s | 1,393 s | 929 unités passent, dont deux régressions entre domaines |
| DATA final Native / Full | 13,392 s / 0 s | 1,021 s / 0,799 s | 600 cas dans chaque mode |
| Analyseur optimisé | 35,763 + 1,322 s | — | Exécutable figé |
| Audit global | 0 s | 10,248 s | +117 complets, +186 capacités, aucune ancienne IR changée |
| Contrats | 2,706 s | 0,732 s | 397 passent |
| Diagnostic indépendant de bibliothèque | 0 s | 0,047 s | Quatre préfixes prouvent l'ordre initial; deux scénarios prouvent ensuite le sort |
| Replay moteur durable | **0 s** | 3,988 s | 735 scénarios et 4 895 assertions passent |
| Preuve verticale Preordain | Aucun nouveau binaire moteur | 11,556 s au total | Trois preuves fraîches |

Les boucles de correction et tous les rapports intermédiaires sont conservés.
La phase « implémentation » de 692,716 s inclut éditions, compilations, tests,
revue et préparation des fixtures; ce n'est pas du temps de saisie pur. La phase
suivante de 700,528 s inclut diagnostic, attente de la lane mana, intégration des
données et certification; elle ne représente pas une exécution de test de
700 secondes. Les durées détaillées priment sur ces fenêtres d'orchestration.

La bibliothèque du harnais est mélangée au démarrage. Son tableau d'entrée ne
garantit pas l'ordre. Les premières attentes supposaient cet ordre et sont
conservées comme échec. Le seed 73 a été validé indépendamment avant scry :
20 assertions de préfixe, puis 21 assertions sur les deux véritables lancements.
Seuls les deux seeds de scénario ont changé, pas les attentes, actions ou IR.

Le moteur Rust n'a pas changé : ses 240 unités CI3 du checkpoint parent restent
applicables avec empreintes de source identiques. Le banc DATA a exécuté les
nouveaux comportements sans recompiler le moteur. Les 145 cartes historiques
restent vertes via 145 preuves de cache valides; ce passage ne les présente pas
comme de nouvelles exécutions.

## Dix sets et suite

Les mêmes dix instantanés locaux passent de 293 à **298 cartes complètes**,
de 1 538 à **1 548 capacités natives**, et de 245 à **250 identités complètes**.
Les 2 368 occurrences et 2 319 identités sont conservées, trois occurrences
vides séparées. Gains complets : HOB +1, MSH +1, TMT +3. Temps interne du
parseur : 0,712 → 0,759 s; processus : 0,955 → 1,076 s. Une seule mesure,
aucun gain de vitesse revendiqué.

Cohorte08 adoptée : 99 cas conservés exactement, Preordain remplacé par
Cauldron of Essence. Audit 0,047 s sans Cargo; 31 capacités natives sur 207.
Les prochains lots sont les alternatives de création de jetons et la défausse
comme action ordinaire. Le mana reste en staging : sa première vraie exécution
donne 291/294 tests verts, avec deux erreurs de quantité et une fixture invalide
à corriger avant intégration. Ses 54 tests compilables n'étaient pas une preuve
d'exécution correcte.

Certificat : `.tmp/night-preordain-keyword-sequence-01/certificate.json`.
Analyseur : SHA256
`6F673723DDF0F75E4F2580A79EB2111FD263F2DF2D4A18E9B75ACB1FB9699F91`.
Timing : `.tmp/card-migration-timings/night-preordain-keyword-sequence-01.json`.
L'objectif global reste actif.
