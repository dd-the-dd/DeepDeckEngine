# Sous-systèmes de règles activables par partie

Ce document décrit une cible d'architecture issue de la revue du 15 novembre
2026. Ce mécanisme n'est pas encore implémenté. Il complète le contrat du
compilateur compositionnel ; il ne remplace pas les preuves de migration.

## Contrat de bout en bout

Le compilateur produit un programme vérifié et ses besoins sémantiques. Le moteur
prépare une partie avec un noyau de règles et les sous-systèmes nécessaires.
Les événements sont distribués aux seuls abonnements pertinents, en respectant
les moments d'évaluation, remplacements et ordres définis par les règles.

Le manifeste est dérivé du programme et des règles intrinsèques, jamais d'une
liste manuelle de noms de cartes ou d'un simple scan de mots. Une mécanique qui
se développe en primitives réutilise leurs besoins ; un état ou événement
intrinsèque conserve son propre contrat. Un nouveau mot-clé n'implique donc pas
systématiquement un nouveau module moteur.

```text
Oracle + connaissances versionnées
  -> AST vérifié
  -> IR exécutable + besoins de capacités
  -> préparation de partie : noyau + dépendances nécessaires
  -> abonnements aux événements et index des règles actives
```

## Trois coûts distincts

- Compilation Rust : les frontières de crates et leurs dépendances déterminent
  les reconstructions. Désactiver une mécanique pendant une partie ne supprime
  pas sa compilation dans le binaire général.
- Compilation Oracle : compiler les définitions nécessaires, puis réutiliser
  leurs programmes immuables via un cache versionné par contenu, compilateur et
  connaissance du domaine. Ne pas compiler un catalogue complet à chaque partie.
- Exécution : éviter les parcours de mécaniques inactives et indexer les règles
  pertinentes par événement/domaine. Mesurer les gains sur des parties et charges
  identiques ; aucune accélération chiffrée n'est actuellement démontrée.

Les modules peuvent être liés statiquement dans le binaire tout en restant sans
état ni abonnement actif dans une partie. Des DLL/plugins dynamiques ne sont pas
un prérequis. Les features Cargo servent aux profils de construction ; elles ne
doivent pas imposer une recompilation pour chaque combinaison de decks.

## Préparation et activation dynamique

Le plan initial couvre les cartes des joueurs, leurs faces et les définitions
associées connues : jetons, copies, cartes nommées et capacités accordées. Les
effets capables d'introduire une définition ultérieure demandent une activation
de capacité avant de publier l'objet ou d'exécuter son programme.

Un besoin inconnu produit une erreur explicite. L'activation doit participer aux
transactions existantes : un échec restaure état, abonnements, index et contexte.
Un index doit être invalidé ou mis à jour lors des changements de zone,
d'identité, de contrôleur et de capacités pertinentes. Il ne peut pas préserver
des résultats de prédicats dont les caractéristiques ont changé.

Ne pas confondre :
- module disponible dans le binaire ;
- module requis potentiellement par une définition ;
- règles et abonnements actuellement actifs ;
- état historique devenu obligatoire pour le reste de la partie.

## Exemple jour/nuit

Le cache officiel local CR 2026-11-06, règles 502.2 et 732.1-732.2, distingue
l'état initial sans désignation et la partie ayant établi le jour ou la nuit.
Dans l'état initial, le contrôle de changement de désignation n'a pas lieu.
Une fois le jour ou la nuit établi, la désignation persiste : retirer le dernier
permanent concerné ne justifie pas de supprimer ce sous-système ni son historique.
Une introduction tardive doit disposer des faits de tour nécessaires au moment
où les règles les demandent ; l'activation ne peut pas inventer un passé vide.

La recherche actuelle trouve daybound/nightbound dans le catalogue officiel,
mais ne démontre pas un sous-système jour/nuit exécuté par le moteur. Cet exemple
fixe le contrat futur ; il ne prouve pas un coût actuel supprimable.

## Points d'entrée actuels à profiler

`src/engine/mod.rs` reconstruit des collections de modificateurs statiques,
parcourt les sources de déclencheurs d'étape et recherche certaines mécaniques
dans les actions basées sur un état. Ces parcours constituent des candidats
concrets à indexer ; leur poids CPU reste à mesurer.

Avant remplacement d'un parcours, vérifier le même résultat observable sur la
voie indexée et la voie de référence : état, événements, décisions, ordre,
activation tardive, disparition de sources, capacités accordées et rollback.
Les règles fondamentales ne deviennent pas facultatives pour gagner du temps.

Les interfaces stables de programme, état, événement et requête doivent guider
le découpage des crates. Créer une crate par mot-clé ou par carte reproduirait la
fragmentation que l'AST cherche à éliminer.
