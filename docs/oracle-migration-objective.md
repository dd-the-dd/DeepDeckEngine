# Objectif : le meilleur moteur de lecture Oracle

## Objectif de référence — précisé le 15 novembre 2026

**Migrer toutes les cartes Magic actuelles, toutes leurs faces et capacités, vers
un compilateur Oracle compositionnel et typé qui produit l'IR primitive commune
de DeepDeck et dont le moteur exécute correctement les règles. Maximiser la prise
en charge des cartes futures par recombinaison des concepts connus, sans nouveau
code pour chaque carte ou recette de phrase.**

La précision d'objectif donnée par l'utilisateur met la **généralisation du langage**
au premier plan : partir des tokens Oracle et des connaissances du domaine pour
construire l'AST, plutôt que chercher une phrase parmi des recettes de cartes.
Comprendre un nouveau texte conforme aux formulations Magic doit demander du code
uniquement s'il introduit un concept ou une règle réellement inconnus. Un corpus
représentatif peut guider ce travail ; il ne remplace pas la vérification finale
de toutes les cartes actuelles, ni les preuves du moteur.

Par exemple, `whenever` ouvre une capacité déclenchée qui attend un événement et
un programme. L'événement peut composer plusieurs événements ; son objet peut
porter un prédicat comme `instant or sorcery`. Ce prédicat n'est pas une nouvelle
famille de déclenchement : acteur, action observée, requête, connecteurs, conditions
et corps d'actions restent des enfants typés réutilisables. Les connaissances de
Magic déterminent leurs domaines, leurs références et leurs moments d'évaluation.

Le lexique est construit une fois et conservé en mémoire. Son dictionnaire trié
permet une recherche binaire ; le dispatch grammatical part ensuite des tokens et
des attentes du contexte. Aucun parcours de toutes les phrases ou regex connues.
Une recherche lexicale coûte O(log V) dans un vocabulaire de V entrées ; cela ne
prouve pas à lui seul une complexité constante du parsing complet. Mesurer aussi
la tokenisation, les tentatives grammaticales, les bindings et l'abaissement IR.

Faire progresser le langage à partir de lots d'échecs réels : analyser les causes
partagées, réparer les contrats de composition ou ajouter les concepts réellement
manquants, prouver le parcours complet, rejouer le lot et les régressions, puis
recommencer jusqu'à couvrir toutes les cartes. Conserver le travail correct déjà
réalisé et traiter explicitement les dettes du moteur après un succès du parseur.

Maintenir une boucle de développement rapide avec des runners pilotés par données,
des tests ciblés à valeur sémantique distincte et des validations approfondies à
la stabilisation des fonctionnalités. Mesurer séparément analyse, implémentation,
attentes, compilation/link, exécution et audits ; ne pas créer une unité Rust par
carte ou par valeur.

Le contrôle principal demandé est la régression de fin de set : rejouer toutes
les cartes du set implémenté et des sets précédemment validés sur des entrées
épinglées. Pendant le développement, privilégier les fixtures DATA du runner
stable ; réserver les unités aux invariants distincts et les validations larges
aux fins de set ou aux changements transversaux. Garder les dettes de couverture
visibles : préserver un set partiel ne signifie pas l'avoir entièrement implémenté.

À chaque nouveau set, mesurer la couverture initiale, identifier les nouveautés
lexicales et mécaniques depuis leurs définitions officielles versionnées, étendre
le modèle partagé, puis comparer couverture et performance sur le même corpus.
Rapporter également ces comparaisons à chaque set terminé, avec les preuves du
moteur et les limites restantes. Garder les décisions et passages de relais dans
le dépôt et les skills afin que chaque agent applique cette même philosophie.

L'ambition du meilleur moteur Oracle se juge sur la justesse des règles, la couverture
native, les diagnostics, la vitesse et la facilité d'évolution ; elle ne constitue pas
une supériorité mesurée sans benchmark comparatif.

## Ce que nous construisons

Un compilateur déterministe du langage Oracle : lexique sans perte → grammaire
compositionnelle → AST sémantique typé → résolution des références, portées et types
→ représentation intermédiaire commune → exécution par le moteur. Les automates
locaux transactionnels servent cette architecture ; ils ne définissent pas des
recettes complètes de cartes.

Le contrat durable et la méthode d'itération sont dans
[oracle-compositional-compiler.md](oracle-compositional-compiler.md).
Une nouvelle combinaison de concepts couverts doit fonctionner sans nouveau code
de production. Les gains de couverture seuls ne démontrent pas cette propriété.
Les primitives correctes et leurs preuves sont conservées pendant cette transition.

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

Toutes les cartes Magic actuelles, recensées dans un corpus Oracle versionné et
identifiées par `oracle_id`, toutes leurs faces et capacités pertinentes, passent
les cinq gates : lexique, syntaxe native, sémantique
primitive, exécution et absence de fallback legacy. Les tokens et emblèmes nécessaires
à ces cartes font partie des règles à couvrir. Les objets non jouables sont comptés
séparément. Le corpus local est utilisé en premier et complété lorsqu'il manque des
cartes ; son contenu présent ne réduit pas le périmètre de la mission. Les nouvelles
cartes publiées rejoignent le corpus avec une version datée.

Les combinaisons de concepts annoncés comme couverts sont démontrées par des
variantes indépendantes sans changement de production. Le protocole d'arrivée
d'un set est réutilisable, et les nouvelles mécaniques gardent leurs paramètres,
sémantiques et événements corrects. Les recettes spécialisées ne sont pas comptées
comme une réalisation de cette généralité, même lorsqu'elles n'utilisent pas de regex.

Un résultat canonical, un contrat exécutable, une capacité seulement native ou un module
non branché ne suffisent jamais à déclarer une carte entièrement migrée. La mission
continue tant que des cartes restent legacy, mixtes, composites, unsupported ou non prouvées.
