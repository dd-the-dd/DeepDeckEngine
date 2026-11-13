# Objectif : le meilleur moteur de lecture Oracle

Réussir à parser **toutes les cartes Magic** et à exécuter correctement leurs règles
dans DeepDeck, avec une architecture qui généralise aux prochaines cartes.
L'ambition du meilleur moteur Oracle se juge sur la justesse des règles, la couverture
native, les diagnostics, la vitesse et la facilité d'évolution ; elle ne constitue pas
une supériorité mesurée sans benchmark comparatif.

## Ce que nous construisons

Un compilateur déterministe du langage Oracle : lexique sans perte → automates
hiérarchiques à mémoire transitoire transactionnelle → représentation intermédiaire
typée composée de primitives → exécution par le moteur.

Les automates partagent les mêmes enfants pour les références et anaphores, les nombres
et variables, les expressions arithmétiques, les cibles et collections, les conditions
booléennes, les coûts, les événements, les zones, les durées et les séquences d'actions.
Les mots `and`, `or`, les virgules et les points prennent leur sens dans leur contexte.
Une référence inconnue, une ambiguïté ou une forme non prise en charge produit un
diagnostic explicite utilisable pour améliorer l'automate.

Un bonus/malus est une action ordinaire : `ModifierCaractéristiques(cible, deltas, durée)`.
Son enfant retourne le nœud et la position du curseur ; le séquenceur poursuit avec
`Dégager`, `Piocher` ou toute autre action. Le parent possède la consommation complète
et transmet les bindings, notamment la cible désignée ensuite par « it ».
Les sorts, activations, déclenchements et chapitres réutilisent ce même corps d'actions.

Les chemins natifs n'emploient ni regex, ni phrase complète reconnue à part, ni
branchement sur un nom de carte, ni opération qui combine artificiellement des effets.
Les primitives moteur correctes déjà présentes sont conservées et composées.

## Boucle de progression

1. **Alterner les sources** : un lot de sets locaux récents, puis la prochaine carte
   non migrée de la pile prioritaire, puis un lot des cartes jouées dans les decks
   publics de DeepDeckLeague ; recommencer dans cet ordre.
   Préserver l'ordre interne de chaque file et ses curseurs. Une carte présente dans
   plusieurs sources n'est migrée qu'une fois, avec toutes ses provenances conservées.
   Les cartes « considering » ne sont pas confondues avec les cartes jouées.
2. **Constituer un lot d'au plus 100 cartes non couvertes** : parcourir les sources
   en alternance et tester les cartes avec le nouveau parseur. Les cartes déjà
   couvertes sont comptées et passent sans réimplémentation. Pour les échecs, conserver
   l'Oracle exact, l'automate, l'état, l'attente, le token rencontré et la mémoire
   transitoire utile au diagnostic. Le lot est figé et identifié pour la comparaison.
3. **Choisir le correctif générique qui débloque le plus de cartes** : regrouper les
   échecs du lot par concept partagé, puis classer les correctifs par nombre de cartes
   qui pourraient devenir entièrement natives. Distinguer capacités touchées, cartes
   encore bloquées par d'autres concepts, exécutabilité et gains réellement mesurés.
   Ne pas additionner les groupes qui se recouvrent. Un test d'isolement reste une
   hypothèse tant que l'intégration n'est pas prouvée.
4. **Confier le meilleur correctif à un agent** : répartir les concepts et fichiers, réserver les points
   d'intégration partagés, faire relire chaque contribution et informer l'équipe lors
   de son acceptation. Garder un seul processus Cargo et geler les sources concernées
   pendant une certification avec fingerprint.
5. **Implémenter au niveau partagé** : modifier l'enfant ou le parent générique qui
   manque, puis réutiliser ce changement sous toutes les enveloppes compatibles.
   Aucun contournement local à la carte représentative.
6. **Prouver le parcours complet** : Oracle exact → reconnaissance native → IR primitive
   correcte → contrat accepté → comportement runtime. Les tests ciblent des différences
   réelles : nominal, généralisation, near-miss et frontière sémantique pertinente.
7. **Reparcourir après chaque correction** : relancer le même lot et les régressions
   avec le même binaire tant que les sources n'ont pas changé. Mesurer combien de
   cartes deviennent entièrement natives, vérifier que les précédentes restent
   couvertes, puis compléter le lot jusqu'à 100 cartes encore non couvertes pour
   l'analyse suivante. Rejouer aussi les corpus touchés et mesurer l'effet de la
   généralisation sur les deux autres sources. Un recul doit être expliqué et corrigé,
   jamais caché par un retour silencieux au legacy.
8. **Rapporter et recommencer** : couverture avant/après, legacy restant, diagnostics,
   gain de cartes entièrement natives et temps séparés d'analyse, d'implémentation,
   de compilation/link, de parsing et de tests. Faire un checkpoint de couverture et
   performance à chaque set terminé et après les changements transversaux importants.

Cette boucle par lots et gain générique remplace l'ordre d'implémentation strict au
premier blocage. L'alternance remplace aussi « finir un set puis seulement passer au
suivant ». Le préfixe historique de la pile reste une régression indépendante : les
cartes issues des autres sources ne gonflent pas artificiellement son avancement.

## Données et cadence

Privilégier le dépôt, les snapshots et l'archive Oracle locale. Importer seulement les
données absentes, conserver leur provenance et réutiliser ensuite le cache. Aucune
publication ni modification du site n'est impliquée par la lecture des decks publics.

Les lots utilisent les runners pilotés par données déjà construits ; l'ajout d'une
fixture ne doit pas exiger un nouveau binaire Rust. Le cache Cargo partagé est
`.tmp/targets/card-migration`. Distinguer systématiquement compilation et exécution.

`scripts/advance-oracle-migration-batch.ps1` conserve les cartes encore non couvertes
du lot précédent et complète les places libérées. Il contrôle l'alignement des
identités entre la file, l'audit et les données Oracle, conserve l'UTF-8 et enregistre
les empreintes des entrées. Les mêmes cartes et textes servent au rejeu avant/après.

Les tests gardent les vrais risques : consommation complète, UNKNOWN, ambiguïtés,
transaction/rollback, cardinalité et légalité des cibles, anaphores, conditions,
arrondis, coût versus effet, ordre des événements, LKI et identité après changement de zone.
Préférer une table de données aux copies de tests Rust. Isoler les anciennes suites
pour la boucle rapide ; retirer un test seulement si sa valeur est remplacée ou s'il
ne couvre plus aucun comportement accessible. Le nombre de tests n'est pas, seul,
un motif de suppression.

La branche contient aussi des fonctions agents, horloges, backend et entraînement.
Ces travaux ne sont pas supprimés au titre du nettoyage de migration. Une branche
legacy active est retirée après remplacement et preuves, avec réduction de dette mesurée.

## Critère de réussite

Toutes les cartes du corpus Oracle local identifié par `oracle_id`, toutes leurs faces
et capacités pertinentes, passent les cinq gates : lexique, syntaxe native, sémantique
primitive, exécution et absence de fallback legacy. Les tokens et emblèmes nécessaires
à ces cartes font partie des règles à couvrir. Les objets non jouables sont comptés
séparément. Les nouvelles cartes publiées rejoignent le corpus avec une version datée.

Un résultat canonical, un contrat exécutable, une capacité seulement native ou un module
non branché ne suffisent jamais à déclarer une carte entièrement migrée. La mission
continue tant que des cartes restent legacy, mixtes, composites, unsupported ou non prouvées.
