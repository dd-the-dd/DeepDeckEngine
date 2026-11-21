# Propriétés numériques des cibles : valeur vivante et LKI

Backend accepté :174 tests privés dont28 nouveaux,653 DATA/4447assertions et
145 cartes historiques/395preuves fraîches passent. Le compteur parser reste
3 568 cartes complètes/17 224 capacités natives : la levée des caps frontend et
le lien Oracle réel→IR→runtime constituent le checkpoint suivant.

Le module target_incarnation conserve la désignation originale. Deux petits
modules ajoutent les valeurs numériques authentifiées et une frame couvrant
l'objet complet en résolution, y compris après son retrait de la pile. Les
objets encore en attente reçoivent leurs snapshots au départ réel de leur cible.
Le service lit l'objet original vivant en priorité ; sinon il lit ses valeurs
effectives avant départ. Une nouvelle incarnation portant le même identifiant
physique ne peut pas remplacer l'ancienne cible.

Les destructions simultanées préparent leurs valeurs depuis le même champ de
bataille, avant le retrait d'un éventuel lord. Une préparation ne publie rien :
seul un départ confirmé engage la LKI. Shield, régénération et indestructible
conservent donc une lecture vivante. Les checkpoints complets clonent la frame ;
le rollback partiel des paiements la restaure explicitement.

La légalité de chaque cible est figée au début de la résolution. Une cible déjà
illégale ne fournit pas d'information, même si d'autres effets restent possibles ;
une cible partie pendant la résolution peut fournir sa LKI. Un contexte absent ou
une identité manquante reste une erreur, jamais une valeur0. Les propriétés
signées restent signées ; seul le résultat final d'un gain/perte de vie est borné
à0. Une instruction de vie0 ne publie ni événement ni déclenchement.

Le scénario public provisoire Feed utilise castSpell puis resolveTopStack :
avantfix, la cible MV5 était détruite mais le lanceur restait20 ; aprèsfix il est15.
Le même fichier est conservé à hash3978E812… ; il ne devient pas une preuve de
parsing tant que l'IR du texte réel n'est pas liée exactement. Le domaine prouvé
est chosenTarget permanent singulier ; contrôleurs, sorts en pile et collections
restent des contrats distincts.

| Passage chronologique | Compilation | Exécution | Résultat |
| --- | ---: | ---: | --- |
| Privés target_numeric |98,962s|1,407s|28/28|
| Partition moteur complète, artefact réutilisé |0,276s de vérification Cargo|0,758s|174/174|
| Runner DATA complet |33,590s|3,806s|653/653,4447assertions|
| Préfixe optimisé historique |voir phases JSON|30,199s total|145/145,395preuves|

Le premier wrapper a supprimé5,98GiB de cache incremental au seuil40GiB avant
compilation ; ce coût et la perte possible de réutilisation sont étudiés dans un
chantier workflow séparé. Aucun cache de dépendances ni source n'a été supprimé.
Benchmark limité aux mêmes34 DATA en un processus : deux échauffements puis sept
paires alternées, médiane0,098460→0,104135s (+5,76%, démarrage inclus). Ce résultat
ne prouve aucune accélération et ne mesure pas une partie complète. La garde
architecture conserve48 primitives, zéro composite, zéro violation/avertissement.

Certificat `.tmp/night-target-lki-root-01/backend-certificate.json`, freeze
`accepted-backend-source-freeze.json`, rapports privés/runtime/strict et
`runtime-benchmark.json` dans le même dossier. Chrono root valide :
`.tmp/card-migration-timings/night-feed-target-lki-root-02.json` ; root-01 est
conservé comme erratum d'identité initiale invalide et exclu des sommes canoniques.
Les temps précédents des agents/frontend ne sont pas rétrodatés ni additionnés
une seconde fois à ces mesures.
