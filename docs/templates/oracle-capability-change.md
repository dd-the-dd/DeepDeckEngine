# Contrat de changement d'une capacité du langage Oracle

Copier cette note dans le dossier de travail du lot. Garder les faits utiles,
retirer les champs sans objet et distinguer hypothèse, implémentation et preuve.
Référence : `docs/oracle-compositional-compiler.md`.

- **Concept et couche manquante :** sans nom de carte ; vocabulaire, grammaire,
  composition/liaison, contrat d'exécution ou nouvelle sémantique.
- **Cas réel :** oracle_id, impression/source, texte/faces complets, hashes,
  préflight de structure, binaire et mode du résultat de référence.
- **Décomposition :** enveloppe → programme → actions → requêtes/expressions.
- **Contrats existants :** types d'entrée/sortie, paramètres, références/portées,
  cardinalités, visibilité, moment d'évaluation et erreurs ; callers réellement lus.
- **Diagnostic :** pourquoi les concepts déjà présents ne se composent pas ; ou
  pourquoi le concept nouveau ne s'exprime pas avec eux.
- **Changement :** réutilisation/extraction proposée ; IR attendue ; ancienne IR
  conservée ou correction intentionnelle ; caps et erreurs explicites.
- **Variantes sans code supplémentaire :** axes pertinents et une combinaison
  non utilisée pendant la conception ; résultats attendus préparés indépendamment
  du parseur ; aucun prétendu succès avant exécution.
- **Preuves :** cas réel/sibling/near-miss, comportement moteur lié à l'IR émise,
  consommation/liaison, corpus affecté et régression ; protocole UI si modifié.
- **Impact :** capacités, cartes complètes et combinaisons ; estimations séparées
  des gains observés ; dettes moteur retenues après succès du parseur.
- **Ownership :** fichiers/boundaries, dépendances entre agents, responsable Cargo,
  état de compilation, application root et risques restant à revoir.
- **Temps et reprise :** chemin du timing JSON, phases/attentes/boucles, dernière
  preuve, commandes exactes et prochaine action vérifiable.

Pour un nouveau mot-clé, ajouter définition officielle/version/provenance,
paramètres, expansion typée ou règle intrinsèque, événements/signaux, quantités
zéro/remplacements, ainsi que le statut réel de chaque couche du catalogue.
