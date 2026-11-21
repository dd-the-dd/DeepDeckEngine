# Séquences de cartes au cimetière et pertes de vie simultanées

Checkpoint **04CA accepté**, après EA91. Le corpus local contient maintenant
**3 638 cartes non vides entièrement parsées et 17 352 capacités natives**,
soit **+13 cartes et +40 capacités**. Le registre de preuves individuelles passe
de 160 à **161**, avec Deathrite Shaman. Les douze autres nouvelles cartes ont
un contrat statique complet vert, mais aucune nouvelle simulation individuelle
n'est revendiquée pour elles.

Certificat : `.tmp/night-deathrite-root-01/certificate.json`.
Analyseur : `.tmp/night-deathrite-root-01/after-analyzer.exe`, SHA256
`04CA8E6D09B201CE0BAC721B075E0B8583386EF6B6B0A6BAA325F98052565944`.

## Ce qui est partagé

Le sous-automate de changement de zone compose le type de carte, les unions de
types et la relation au cimetière. Le parent de séquence reçoit un nœud typé et
compose ensuite les actions ordinaires d'ajout de mana ou de changement de vie.
Il n'existe aucune opération combinée propre à Deathrite.

L'annotation « Activate only as an instant » est reconnue et validée par son
parent d'activation. Elle n'est pas supprimée comme une parenthèse quelconque.
Une capacité ciblée qui ajoute du mana reste une capacité utilisant la pile.
Les trois anciennes IR atomiques d'exil au cimetière restent exactes.

`loseLife` accepte directement un ensemble de joueurs. Le moteur prépare tous
les destinataires et montants, modifie tous les totaux, inscrit tous les
événements, puis notifie les observateurs. Le véritable `forEachPlayer` conserve
son exécution séquentielle. Les actions basées sur l'état restent à la frontière
de résolution habituelle : une perte suivie d'un gain peut donc être terminée
avant l'élimination. L'appel direct `execute_canonical_rule` laisse cette dernière
étape à son appelant.

`resolution_preflight.rs` possède l'unique parcours récursif des programmes et
de la pile pour déterminer si un checkpoint d'annulation est nécessaire. Chaque
domaine fournit un prédicat local. Les tests prouvent la restauration de l'état
sur erreur et une reprise identique à une exécution sans erreur.

## Couverture et régressions

| Mesure | EA91 | 04CA |
| --- | ---: | ---: |
| Cartes non vides entièrement natives, corpus global | 3 625 | 3 638 |
| Capacités natives, corpus global | 17 312 | 17 352 |
| Cartes complètes, occurrences dans les dix sets | 287 | 289 |
| Capacités natives dans les dix sets | 1 525 | 1 531 |
| Cartes complètes uniques dans les dix sets | 239 | 241 |
| Cartes complètes non vides dans le pool de 750 | 171 | 172 |
| Cartes complètes dans la cohorte active de 100 | 0 | 1 |
| Cartes avec preuves individuelles enregistrées | 160 | 161 |

Les 39 288 identités ordonnées du corpus sont conservées. Parmi les anciennes
capacités natives, **17 243 IR restent exactes** et **69 reçoivent uniquement la
transformation documentée** de boucle de perte de vie vers perte simultanée.
Le comparateur refuse les champs supplémentaires, les changements de casse ou
de type JSON et toute capture de la variable de boucle supprimée. Il n'existe
aucun autre changement d'ancienne IR native ni perte de carte complète.
Les cinq changements d'ancienne IR dans les dix sets appartiennent à ces 69.

Nouvelles cartes complètement parsées : Heritage Reclamation, Crossroads
Candleguide, Deathrite Shaman, Rats' Feast, Ambush Wolf, Startled Relic Sloth,
Grave Robbers, Thraben Heretic, Griffnaut Tracker, Return to Nature, Creakwood
Ghoul, Glorious Decay et Soul-Guide Gryff.

Vérifications acceptées : 897 tests unitaires du parseur, 11 tests du service de
perte de vie, 181 tests moteur de la partition 3, 546 cas DATA Native et 546 Full,
390 contrats complets, 687 scénarios moteur avec 4 660 assertions. Le contrôle
strict historique des 145 cartes réexécute 395 preuves; Deathrite réexécute un
contrat et huit scénarios sur ses trois IR réellement émises. Aucun accès réseau.

## Temps et boucles

Le suivi root couvre **872,812 secondes** d'intégration, vérifications et
coordination; il ne mesure pas seulement l'écriture du code. L'analyse et le
développement initial de l'agent sont enregistrés séparément et peuvent se
chevaucher avec d'autres travaux. Ne pas additionner ces durées comme une mesure
de temps écoulé.

| Étape root, ordre d'exécution | Construction Cargo | Exécution des tests | Description |
| --- | ---: | ---: | --- |
| Tests ciblés de perte de vie | 52,62 s | 1,01 s | 11 cas, dont simultanéité et annulation |
| Première construction des tests du parseur | 10,94 s | 0 s | Échec : import manquant dans le fichier de tests |
| Reprise après correction de l'import | 16,31 s | 1,42 s | 897 tests verts |
| Runner DATA du parseur | 12,67 s | 1,21 s | Premier passage diagnostique : un chemin mal saisi et l'ancienne attente IR de perte de vie |
| Analyseur optimisé | 29,60 s + 1,28 s de liaison | — | Artefact figé pour les audits |
| Runner DATA moteur | 31,14 s | 3,64 s | 687 scénarios, dont 16 nouveaux |
| Partition moteur 3 | 122,89 s | 1,91 s | 181 tests, reconstruction unique du binaire |
| Contrôle strict historique | 11,17 s | 7,14 s | 395 preuves; 36,14 s au total avec contrôles et orchestration |
| Contrats anciens et treize nouveaux | 0,22 s, artefacts déjà frais | 0,22 s | 390 contrats, aucun Rust recompilé |
| Contrôle strict Deathrite | 1,45 s, caches chauds | 0,22 s | Neuf preuves; 10,99 s au total |

Les durées de construction sont les durées Cargo mesurées par les wrappers,
incluant leur liaison, et non une estimation du seul travail du compilateur.
La première tentative du wrapper de contrats a refusé une métadonnée périmée;
le passage avec `-Build` a vérifié les artefacts sans recompiler du Rust.
Les reprises DATA finales ne compilent rien : Native 0,683 s, Full 0,660 s,
687 scénarios moteur 3,025 s. Les douze nouveaux cas parser seuls prennent
environ 0,052 s de processus. L'audit global des 39 288 objets prend 10,442 s.

Le suivi de l'agent contient quatre boucles correctives, dont l'import corrigé
par root; ce dernier apparaît aussi dans le suivi root et ne doit pas être
compté deux fois. Des corrections de préparation des fixtures et de scripts
restent dans les journaux; elles ne constituent pas des corrections de grammaire.

Benchmark sur les mêmes 2 368 occurrences, un échauffement puis cinq paires
alternées : médiane du parseur **0,628 → 0,647 s (+3,03 %)**, processus complet
**0,751 → 0,768 s**. Une régression Cargo pouvait partager le processeur.
Ce lot augmente la couverture; il ne démontre aucun gain de vitesse de parsing
ni de simulation.

Suivis : `.tmp/card-migration-timings/night-deathrite-root-01.json` et
`.tmp/night-deathrite-graveyard-01/timing/night-deathrite-graveyard-01.json`.
Comparaisons : `.tmp/night-deathrite-independent-review-02/real-replay-summary.json`.

## Limites conservées

Les destinataires aléatoires et l'agrégation « un ou plusieurs joueurs perdent
de la vie » restent refusés explicitement faute de contrat partagé prouvé.
Reflecting Pool et le gros lot de dommages restent en préparation séparée.
Leurs propositions ne comptent pas comme des cartes migrées. La migration de
toutes les cartes reste active; le prochain lot conserve les cartes non résolues
et est choisi par impact d'un correctif générique sur les cartes complètes.
