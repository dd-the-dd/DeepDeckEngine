# Contrat du compilateur Oracle et stratégie de progression

Ce document fixe les décisions d'architecture et de travail discutées avec
l'utilisateur le 15 novembre 2026. Il décrit la cible et les critères de revue ;
il ne certifie pas que le code les respecte déjà. Les preuves et les travaux
inachevés restent dans `oracle-migration-current.md`.

## Résultat attendu

Comprendre toutes les cartes Magic actuelles, toutes leurs faces et leurs règles,
et réutiliser les concepts connus pour les cartes futures. Une nouvelle mécanique
peut exiger une nouvelle sémantique. Une nouvelle combinaison de concepts couverts
doit fonctionner sans nouveau code de production.

L'unité de développement est une capacité du langage, prouvée par des cartes.
L'absence de regex ou de branche sur un nom de carte ne suffit pas à établir la
généricité : une recette de phrase réécrite en automate reste une recette.

## Modèle commun

Compiler : structure de carte → limites logiques des capacités → tokens avec
spans → grammaire compositionnelle → AST sémantique typé → résolution des noms,
portées et types → IR commune → exécution et protocole de décisions.

Les automates locaux, la descente récursive et les lecteurs de précédence sont
des moyens de construire ce modèle. Le choix d'un outil de parsing ne doit pas
déterminer les opérations du moteur. Garder la tokenisation unique, les enfants
transactionnels et les diagnostics structurés existants.

La lecture est dirigée par les attentes du contexte et les connaissances du
domaine, sans comparaison à un catalogue de phrases. Le lexique partagé fournit
des formes et des catégories ; la grammaire décide leur rôle. Un `whenever`
attend un événement et un programme, `cast` attend notamment un acteur et un sort,
et `instant or sorcery` compose le filtre de ce sort. Ne pas créer un déclencheur
distinct pour chaque combinaison de ces paramètres. Les conditions d'événement,
les conditions intervenantes et les conditions du programme ont des moments
d'évaluation différents : les représenter explicitement, sans les confondre.

Un sous-automate peut lire un sujet avant de connaître le verbe. Cette lecture
reste tentative : une erreur de binding ne devient engagée qu'une fois reconnue
une action que cet enfant possède. Si le verbe relève d'un autre enfant, rendre
`NoMatch` et restaurer le curseur, même si un préfixe a été consommé. Ne pas
intercepter ainsi un contrôleur de recherche ou une autre portée légitime.

Le dictionnaire lexical trié et réutilisé évite un scan de tous les concepts ;
il ne dispense pas de structurer le dispatch grammatical. Une extension de
vocabulaire ne doit pas multiplier les tentatives sur les anciens textes.
Conserver une preuve de recombinaison inédite sans changement de production et
mesurer les échecs par contrat manquant, pas par nom de carte représentative.

| Domaine | Contrat réutilisable |
|---|---|
| Vocabulaire | Identifiants canoniques, formes/pluriels, catégories et version ; un mot connu ne garantit pas sa sémantique |
| Référence | Source, contrôleur, propriétaire, cible ou résultat lié, avec portée et identité |
| Requête | Domaine, zone, relations et prédicat composable ; collections ordonnées si nécessaire |
| Expression numérique | Littéral, variable, propriété, comptage/agrégation et arithmétique checked |
| Condition | Comparaisons, booléens et quantificateurs sur les requêtes partagées |
| Action | Arguments typés, résultats liés, effets observables et erreurs explicites |
| Programme | Séquence, condition, choix facultatif, itération, portée des résultats |
| Capacité | Contexte d'exécution, coûts/déclarations, événement, remplacement ou effet continu |

## Algèbre de requêtes partagée

Une requête est une expression pure et typée : domaine d'entités, ensemble source
(objets autorisés, objets d'un événement ou résultat lié), relations et prédicat. La sélection
de cartes, permanents ou joueurs doit réutiliser cette même algèbre dans les coûts,
cibles, effets de groupe, choix, comptages, conditions et filtres d'événements.
Les vues de définition imprimée et de caractéristiques effectives adaptent l'accès
aux propriétés ; elles ne dupliquent ni les noms de sous-types ni la logique des
prédicats. Un joueur ne reçoit pas artificiellement les propriétés d'une carte.

- Propriétés partagées : types, sous-types, couleurs, capacités et paramètres de
  capacités, force, endurance, coût de mana typé, valeur de mana et états pertinents.
  Coût de mana et valeur de mana restent deux propriétés distinctes. Les relations
  de propriétaire, contrôleur, source et joueur d'événement sont des nœuds explicites.
- La zone est un prédicat de première classe : ZoneIs/ZoneIn sur des identifiants
  de zones, composable avec les autres critères et leurs relations de joueur.
  Une requête peut couvrir battlefield, hand et graveyard ensemble, sans trois
  grammaires ou trois implémentations de filtres. L'énumérateur peut pousser ce
  critère vers les index/conteneurs pour ne visiter que les zones utiles ; cette
  optimisation conserve le sens de la requête. Autorisation de lecture, visibilité,
  identité et vue imprimée/effective restent des contrats explicites. Une sélection
  multizone ne transforme pas une recherche, un ciblage et un choix en la même action.
- Composition structurelle : And/Or à nombre arbitraire d'opérandes, Not, comparaisons
  et quantificateurs. Une virgule n'est pas un opérateur global : le lecteur de liste
  grammatical détermine sa portée. Ne pas créer un filtre pour chaque combinaison.
- Opérandes numériques symboliques : une propriété se compare à un littéral, une
  variable ou une expression partagée, sans seconde grammaire numérique locale.
- Consommateurs distincts : désigner des cibles, choisir pendant la résolution,
  prendre tous les résultats, compter, tester une présence ou agréger une propriété.
  Le prédicat est partagé ; quantité, sélecteur, caractère facultatif, visibilité,
  ordre et règles de ciblage restent dans leurs contrats respectifs.
- Contexte explicite : zone, identité d'objet, portée des liaisons, caractéristiques
  imprimées/effectives et moment d'évaluation. Les requêtes d'événements conservent
  l'objet et le reçu de cet événement ; une référence à la source ne les remplace pas.
- Validation : une propriété inconnue ou inapplicable au domaine, une capacité
  d'évaluation manquante ou une liaison absente produit un diagnostic contextualisé.
  Ne jamais transformer ce manque en faux, zéro, collection vide ou absence de cible.
  Les paramètres doivent être validés même sur ensemble vide ; conserver ensuite
  les politiques établies de présence/témoin et d'évaluation conditionnelle.
- Le moteur produit les candidats légaux et le protocole de décision. L'UI et l'IA
  consomment ce protocole ; elles ne réimplémentent pas les filtres de règles.

Cette section fixe une cible et une exigence de composition, pas un inventaire de
couverture. Un nouveau champ nécessite encore sa sémantique et ses preuves. Une fois
supporté, il doit se recombiner avec les lecteurs et consommateurs compatibles sans
nouvelle opération par phrase ou par carte.

Les expressions dans les paramètres restent symboliques et sans effets de bord.
Le moteur les évalue au moment défini par les règles. Les actions qui modifient
la partie restent dans le programme. Le compilateur attribue les identifiants de
liaison : deux `look` imbriqués ne doivent pas partager accidentellement un nom fixe.

Exemple de conception, pas de nouvelle syntaxe IR déjà implémentée :

```text
vues = look(player, top(library(owner), N))
choisies = choose(player, from=vues, where=P, quantity=Q)
move(choisies, destination=Z)
```

Changer N, P, Q ou Z réutilise les mêmes enfants et opérations, dans les limites
de leurs contrats. La grammaire porte les différences obligatoires/facultatives,
les cartes restantes, la visibilité et l'ordre. Une cible désignée au lancement
et un choix pendant la résolution gardent leurs règles distinctes. Le moteur
calcule les choix légaux ; l'UI réutilise le protocole et ne recode pas les filtres.

## Relations de joueur dans les requêtes de zones

Une zone personnelle porte une relation vers son détenteur : le joueur dont il
s'agit du cimetière, de la main ou de la bibliothèque. Cette relation ne teste
ni le propriétaire ni le contrôleur des cartes contenues dans cette zone.
Réutiliser les références et ensembles de joueurs partagés pour calculer cette
population ; ne pas ajouter un lecteur de `your` propre à chaque zone ou effet.

Décomposition sémantique (notation de conception, pas une promesse de couverture
de chaque forme dans l'IR actuelle) :

| Texte / intention | Population des détenteurs | Consommateur |
|---|---|---|
| `your graveyard` | joueur de référence du contexte de la capacité | requête de cartes dans les cimetières de cette population |
| `all graveyards` | tous les joueurs admissibles | même requête |
| `an opponent's graveyard` | adversaires du joueur de référence, puis choix si nécessaire | même requête après liaison du choix |
| `target player's graveyard` | référence au joueur ciblé et déclaré | même requête après validation du ciblage |
| choisir deux cimetières | références de deux zones choisies parmi une population légale | même requête après validation de l'ensemble choisi |

Le contexte définit le joueur de référence : notamment le contrôleur de la
capacité ou le joueur lançant le sort. Il ne faut pas substituer automatiquement
le propriétaire de la carte source. Les références d'événement, de cible et de
choix conservent leur identité, leur portée et leur moment d'évaluation.

Une requête pure ne déclenche aucun choix. Une cardinalité de deux appartient à
la déclaration ou à l'action de choix, pas au prédicat d'appartenance à une zone.
Choisir des zones et cibler leurs détenteurs sont des opérations distinctes :
ne pas inventer un ciblage de joueur pour émuler un ciblage de zone. Une forme
sans producteur typé supporté produit un diagnostic explicite.

Les comptages consomment la collection obtenue, avec déduplication des identités
d'objets lorsque plusieurs branches ou zones se recouvrent. Une réduction de
coût utilise le même langage de prédicats pour filtrer les sorts auxquels elle
s'applique et pour calculer son montant via une requête ; leurs sujets et leurs
contextes restent distincts. Un filtre ou une liaison invalide doit échouer même
si les zones sont vides, sans devenir implicitement un comptage de zéro.

## Protocole commun de sélection pour l'UI et l'IA

Le moteur expose une demande de sélection d'entités commune : identifiant de
demande et révision, joueur qui choisit, références autorisées des candidats,
quantités minimale/maximale, ordre éventuel et permissions de confirmation ou
de refus. Les références identifient les objets de partie dans leur incarnation
et leur contexte de zone ; un oracle_id ou un nom de carte ne suffit pas.

Un composant UI commun utilise ces références pour surligner les cartes/permanents
ou joueurs éligibles, afficher les zones concernées, conserver la sélection et
soumettre les identifiants au moteur. Il ne relit pas Oracle et ne recode pas les
filtres de type, couleur, zone, force ou capacité. Les modes/nombres peuvent garder
leurs contrôles adaptés au même protocole de décisions.

Le contrat distingue éligibilité individuelle et validité de l'ensemble choisi.
Des contraintes comme des noms différents ou une somme de forces s'évaluent dans
le moteur. Le protocole permet de soumettre une sélection partielle et de recevoir
les candidats actuellement éligibles ainsi que canConfirm ; cela ne promet pas
une énumération préalable de toutes les combinaisons possibles. Une demande de
choix facultative et une cardinalité minimale de zéro gardent leurs sens distincts.

L'autorité finale reste côté moteur : vérifier la demande, sa portée/révision,
les identités et l'ensemble complet avant paiement ou effet. Une sélection périmée
ne devient pas silencieusement une autre sélection. Les informations visibles et
les candidats transmis sont projetés pour le destinataire autorisé ; ne pas
diffuser des détails de main/bibliothèque cachés à tous les clients. Le choix de
cibles à la déclaration et le choix pendant la résolution partagent l'UI, tout en
gardant leurs moments, contraintes et contextes d'exécution.

Ce protocole est une cible d'architecture à réconcilier avec les types de décision
déjà présents. Cette section ne certifie pas une nouvelle implémentation UI.

## Décider quoi corriger lorsqu'une carte échoue

1. Garder l'Oracle complet et sa structure réelle ; reproduire sur un binaire
   identifié. Distinguer lexique, grammaire, liaison/type, abaissement IR, contrat,
   exécution et présentation. Une limite moteur n'est pas une nouvelle règle Magic.
2. Écrire la décomposition souhaitée en concepts sans le nom de la carte.
   Si ces concepts existent, chercher d'abord le défaut de composition, de portée
   ou de transport des paramètres. Ne pas ajouter un parent pour la combinaison.
3. Lire les fonctions complètes et leurs callers avant d'affirmer une absence.
   Identifier le contrat existant, les erreurs et les consommateurs ; une recherche
   textuelle sans résultat n'est pas une preuve qu'une fonctionnalité manque.
4. Classer le changement : vocabulaire/donnée, production grammaticale,
   composition/liaison, capacité d'exécution, ou véritable concept sémantique nouveau.
   Ajouter une primitive seulement si les opérations et contrôles existants ne
   peuvent pas exprimer le comportement sans perte.
5. Définir le contrat et les variations qui doivent ensuite marcher sans modifier
   la production. Utiliser `templates/oracle-capability-change.md` pour le passage
   de relais ; une note courte suffit pour un changement simple.

Les échecs sont regroupés par domaine sémantique manquant et contrat d'enfant,
avec les diagnostics comme indices. Deux états de parseurs spécialisés différents
peuvent révéler la même absence de requête ou de liaison.

Exemples de décisions attendues, pas un état de couverture du code actuel :

| Nouvelle difficulté | Décision de l'agent |
|---|---|
| Regarder trois cartes plutôt que quatre, puis filtrer les éphémères | Réutiliser quantité, requête, choix et déplacement ; réparer leur composition si nécessaire |
| Employer le même comptage dans des dégâts et un gain de vie | Transmettre la même expression numérique aux deux actions |
| Ajouter un sous-type et son pluriel, sans nouvelle règle intrinsèque | Étendre le vocabulaire partagé et ses formes, puis vérifier tous les consommateurs concernés |
| Rencontrer un mot-clé inconnu | Lire sa définition versionnée, classer sa sémantique, puis composer ou étendre un domaine |
| Ajouter un coût et un effet utilisant tous deux un sacrifice | Partager sélection/opération ; conserver les règles distinctes de paiement et de résolution |

## Boucle rapide et qualité

Conserver la rotation des sources, les lots de 100, leur provenance et le classement
des correctifs décrits dans `oracle-migration-objective.md`. Seuls les correctifs
qui satisfont ce contrat compositionnel sont admissibles au classement.

1. Rejouer le lot figé avec les runners chauds ; conserver aussi les dettes moteur
   des cartes déjà parsées. Séparer gains observés et potentiel supposé.
2. Choisir un concept partagé, un cas réel et une preuve verticale minimale.
   Attribuer les fichiers à des propriétaires distincts ; le root intègre et relit.
3. Prouver d'abord le composant et ses interfaces, puis le parcours Oracle réel
   → IR exacte → comportement. Tester au moins une combinaison de concepts connus
   non utilisée pour concevoir le correctif, sans modifier la production pour elle.
   Au checkpoint, le relecteur choisit cette combinaison après gel du correctif
   et la fait exécuter sur le même binaire vérifié. Un échec rouvre le contrat ;
   ne pas affaiblir l'attente pour conserver un résultat vert.
4. Couvrir les axes pertinents : quantité littérale/expression, prédicat,
   source/contrôleur/propriétaire, zone, connecteur ou contexte. Choisir des variantes
   qui distinguent des sémantiques ; ne pas multiplier tout le produit cartésien.
5. Rejouer les régressions affectées, puis les corpus au checkpoint. Documenter les
   nouvelles combinaisons sans code, les caps et les divergences IR. Une correction
   intentionnelle de vieille IR est explicitement décrite, jamais cachée.
6. Recharger le lot en gardant l'ordre des non-résolus. Actualiser le suivi courant
   avec décisions, propriétaires, dernière preuve et prochaine action vérifiable.

Utiliser les scripts existants : `track-card-migration.ps1`,
`run-parser-fixture.ps1`, `run-contract-fixture.ps1`, `run-runtime-fixture.ps1`,
`test-card-migration.ps1`, `advance-oracle-migration-batch.ps1` et
`audit-card-set.ps1`. Suivre leurs arguments réels et les règles d'AGENTS.md.
Pour enchaîner des preuves, réutiliser `run-capability-proof.ps1` avec un plan JSON :
il appelle ces wrappers séquentiellement, conserve leurs rapports et mesure les
temps sans les additionner deux fois. Une construction exige la réservation de
la file Cargo. Son résultat valide l'exécution des preuves, pas leur pertinence
sémantique ni la migration d'une carte ; la liaison Oracle → IR → moteur reste
une responsabilité de revue.
Une modification de fixture ne doit pas relancer Cargo. Reconstruire la partition
nécessaire après changement de source ; une seule lane Cargo, sources stables
pendant le build. Un changement de grammaire n'impose pas une nouvelle UI si le
contrat de décision est identique. Tester le protocole si ce contrat change.

Avant de geler un paquet, vérifier le style de fins de ligne de chaque candidat
contre sa préimage en octets. `rustfmt --check` ne prouve pas cette conservation.
Déduire le choix Unix/Windows de chaque fichier d'origine, puis le demander
explicitement au formatage ; ne pas appliquer un style global au staging. Une
conversion involontaire de tout le fichier exige une correction bornée avant
intégration, pas une nouvelle compilation ni une réécriture des preuves gelées.

Le passage chaud d'audit/classement vise cinq minutes par lot. Ce n'est pas une
promesse de cinq minutes pour une nouvelle règle moteur. Séparer analyse,
implémentation, intégration/attentes, compilation/link, exécution et audit. Si
l'analyse tourne en rond, produire le contrat/callgraph manquant et resserrer la
tâche ; ne pas recommencer une exploration déjà documentée.

Le catalogue de mots et mécaniques devient un paquet de langage versionné.
Un ajout de données à un schéma déjà exécutable devrait à terme se charger dans
un runner stable. Aujourd'hui des catalogues utilisent `include_str!` : ils peuvent
encore exiger une compilation. Ne pas annoncer un rechargement dynamique inexistant.

## Choisir les tests pour une boucle courte

Un test est retenu pour le risque sémantique distinct qu'il détecte. Le nombre de
tests et le nom du fichier ne définissent pas sa valeur ni sa nature unitaire.
Un test qui initialise tout le moteur, résout une partie ou explore un grand
espace de choix est une preuve d'intégration ou de robustesse, même dans `--lib`.

| Niveau | Usage | Contenu |
|---|---|---|
| Boucle locale | Pendant les modifications | Fixtures DATA dans le runner stable ; unités seulement pour un invariant nouveau ou un défaut précisément isolé |
| Régression du domaine | À stabilisation du correctif | Cas sémantiques distincts, composition, contrat et comportement concernés |
| Validation approfondie | Avant acceptation de la fonctionnalité | Séquences longues, limites réelles d'exploration, interactions complexes et corpus affectés |
| Régression principale par set | À la fin de chaque set implémenté | Set entier et tous les sets précédemment validés, préservation des anciens IR, contrats et comportements concernés, couverture et performance |

### Preuve d'exécution des cartes supportées (smoke runtime, gate optionnel)

Une carte « supportée » (canonique et exécutable) peut encore ne rien faire en
jeu. `runtime_smoke` (`src/engine/tests/runtime_smoke.rs`) est une preuve de
robustesse hors boucle rapide, exécutée explicitement à la validation
approfondie et à la régression par set ; sans les variables `SMOKE_*`, le test
rend la main immédiatement (`SMOKE_FILTER` restreint un diagnostic à quelques
noms de cartes) :

```text
SMOKE_INPUT=.tmp/released-vs-main/corpus-input.jsonl SMOKE_PROBE=<sortie du probe de support> SMOKE_OUTPUT=<rapport jsonl> cargo test [--release] -p deepdeck-engine-runtime --lib runtime_smoke -- --nocapture
```

Pour chaque carte supportée du probe et chaque règle de sort, activée ou
déclenchée, il construit un plateau synthétique (de chaque côté : ours vert,
géant noir, volant blanc, jeton créature, artefact, enchantement, terrain de
base et terrain non-base ; créature, éphémère et terrain en main ; créature,
rituel et artefact au cimetière ; bibliothèque de types mêlés ; réserve de
mana pleine) et emprunte le chemin réel quand il existe :

- `cast` : le sort est lancé par l'action légale (coûts payés par le moteur,
  X = 2 quand X est déclaré, la déclaration au plus de cibles) ;
- `activate` : la capacité est activée par l'action légale de sa règle
  (`abilityRuleIndex`), coûts de mana, d'engagement, de sacrifice, de défausse
  ou de vie compris ;
- `event` : l'événement déclencheur réel est produit (la source lancée depuis
  la main pour « enters », une autre créature lancée, une mort par dégâts
  mortels, le début d'une étape, un éphémère lancé par soi ou l'adversaire) ;
  un événement produit sans que la capacité se déclenche est classé
  `noTrigger` ;
- `combat` : un combat réel (attaque de la source, blocage par ou de la
  source) ; la capacité est jugée sur la fenêtre d'événements de sa
  résolution ;
- `synthetic` : à défaut, la déclaration est faite directement pour la règle
  (événement synthétique pour un déclencheur).

Le résultat est classé `error` (erreur ou panique), `noTrigger`, `noChange`
(une primitive de premier niveau qui promet un changement observable —
dégâts, vie, zones, marqueurs, jetons, engagement, F/E — n'a rien changé),
`wrongCount` (le changement atteint un autre nombre d'objets que les cibles
déclarées, une collection change un autre ensemble que celui de la requête
d'objets partagée, ou deux emplacements « another target » nomment le même
objet), `uncovered` (non installable, avec sa raison : règle statique ou de
remplacement, action légale absente sur ce plateau, contexte non fourni,
montant ou collection vide, programme conditionnel qui n'a rien changé) ou
`pass`. Un programme conditionnel ou à choix est classé à part,
`contingentPass`, quand l'une des primitives qu'il contient a changé un objet
ou un joueur autre que sa source ; un déplacement de zone ne compte que pour
une carte autre que la source. Un sort non permanent se résout comme
une carte lancée ; ses propres déplacements et ceux de la source ne comptent
pas comme l'effet. `uncovered` n'est jamais compté comme une réussite. Une
règle que le runtime refuse à la résolution alors que le validateur l'accepte
est un défaut d'alignement : le validateur doit la refuser avant le jeu (la
carte cesse alors d'être supportée). Le rapport donne les comptes par classe,
par chemin et par primitive d'IR ; une hausse de `error`, `noTrigger`,
`noChange` ou `wrongCount` bloque l'acceptation. `SMOKE_PROGRESS` écrit chaque
règle jouée sur la sortie d'erreur pour localiser un scénario qui ne termine
pas.

La régression principale est pilotée par les cartes réelles, pas par une nouvelle
suite unitaire compilée à chaque carte. Épingler les mêmes entrées avant/après et
réutiliser les runners compilés tant que leur code ne change pas. Conserver les
checkpoints transversaux d'AGENTS.md pour détecter tôt une régression commune ;
ils ne remplacent pas la validation finale par set.

Un set encore partiellement couvert constitue une référence de non-régression,
pas un set terminé. Séparer les cartes entièrement compilées, les capacités
partielles, les grammaires non prises en charge et les comportements moteur prouvés.
Préserver aussi les cartes déjà couvertes dans les autres corpus : attendre leur
fin de set pour découvrir une perte n'est pas nécessaire. Une réussite de parsing
ne démontre pas, à elle seule, l'exécution correcte de toute la carte.

Pour chaque nouveau test, indiquer le comportement qui deviendrait incorrect s'il
était absent et la preuve existante qui ne le couvre pas. Ne pas créer une unité
Rust par carte ou par valeur numérique. Une variante de données peut réutiliser
un test paramétré ; une carte composée de concepts déjà prouvés nécessite la
preuve de sa vraie composition, pas une copie des unités de chaque primitive.
Les catégories nominal/sibling/near-miss/runtime peuvent réutiliser des preuves
pertinentes : elles n'imposent pas quatre nouveaux binaires ou fonctions de test.

Les unités doivent isoler une petite responsabilité et utiliser des entrées
petites. Pour une limite algorithmique, une borne interne injectée à petite
échelle peut prouver la logique ; le test de la vraie borne de production reste
dans la validation approfondie. Ne jamais modifier les limites Magic ou de
production pour faire passer la boucle rapide. Ne pas masquer un test coûteux
avec `ignore` sans une commande/gate réelle qui l'exécute à la bonne étape.

Reclasser un test lent avant de le supprimer. Fusionner les doublons seulement si
les assertions et erreurs distinctes restent couvertes. Supprimer un test miroir
de l'implémentation, supersédé ou sans chemin accessible avec une justification
et une preuve de remplacement si nécessaire ; ne pas diminuer la validation des
liaisons, cibles, zones, paiements, événements ou rollbacks pour gagner du temps.

Mesurer compilation/link, préparation du scénario et exécution séparément avant
d'optimiser. Une unité exécutée en millisecondes peut encore entraîner une coûteuse
compilation d'un gros crate. Séparer des fichiers ne crée pas une frontière Cargo :
réutiliser les runners DATA et isoler les modules/crates utiles lorsque nécessaire.
Une régression rapide reste rapide tant que les mêmes binaires vérifiés sont
réutilisés ; ne pas répéter les gates larges déjà verts sans changement pertinent,
échec nouveau ou risque non résolu. Les changements transversaux conservent leur
validation complète avant acceptation, à la cadence d'AGENTS.md.

Les objectifs de durée servent au diagnostic, pas de seuils muraux fragiles dans
les assertions de justesse. Les futurs découpages de suites doivent documenter les
commandes de chaque niveau ; cette politique n'implique pas que le découpage actuel
soit déjà réalisé.

## Arrivée d'un set et nouveaux mots-clés

1. Privilégier les données locales ; importer seulement ce qui manque. Figer liste
   des cartes, faces, textes, identifiants, versions et hashes. Un Oracle modifié
   pour un oracle_id existant doit être réaudité ; ne pas le supprimer par déduplication.
2. Faire le premier audit AVANT toute adaptation du set. Mesurer cartes/capacités
   complètes, partielles et bloquées, y compris les contrats et les dettes moteur.
3. Différencier les nouveautés lexicales : nom/référence, type/sous-type, compteur,
   mot de capacité sans règle intrinsèque, action mot-clé ou capacité mot-clé.
   Un UNKNOWN ne devient pas un mot générique pour faire disparaître le diagnostic.
4. Pour une mécanique, conserver sa définition et sa version depuis les règles
   officielles ou notes de sortie, en utilisant le cache si présent. Le reminder
   et une paraphrase LLM ne sont pas une spécification complète. Lire les sources
   officielles manquantes avant d'implémenter une règle incertaine.
5. Exprimer ses paramètres et sa sémantique avec les requêtes, expressions,
   programmes, modificateurs, événements et remplacements existants. Un mot-clé
   n'est pas forcément une suite d'actions immédiates. Préserver son identité et
   sa politique d'événements pour les déclencheurs : moment, quantité zéro,
   remplacement, résultats et répétition selon les règles. Ne pas émettre un
   signal simplement parce que le mot apparaît dans le texte.
6. Si la mécanique requiert un concept nouveau, étendre le domaine commun puis
   le catalogue. Une entrée lexicale ou un template déclaré ne vaut pas exécution.
7. Rejouer le MÊME set et les régressions concernées. Rapporter avant/après,
   temps, règles restantes et versions. Un set est terminé seulement quand toutes
   ses capacités pertinentes satisfont les gates ; aucune nouveauté n'est ignorée.

Un changement du compilateur, du moteur ou du paquet de langage invalide les
caches concernés. Un audit incrémental des seules nouvelles cartes ne remplace
pas les régressions après modification d'un composant partagé.

## Mémoire durable et revue

La doctrine vit ici. L'objectif et les règles locales pointent ici. Les skills
orientent les agents vers ces fichiers du dépôt, partageables entre clones.
`oracle-migration-current.md` garde un résumé compact des faits actuels, y compris
une branche non compilable et la réparation prioritaire, sans recopier l'historique
des conversations. Les contrats et preuves des lots restent liés depuis ce suivi.
Conserver les analyses déjà vérifiées tant que leurs sources et contrats sont
inchangés ; relire les frontières modifiées lors d'une intégration. Un agent ou un
build noté dans un ancien suivi n'est pas une preuve de processus encore actif.

Vérifier l'état réel d'un agent avant d'attribuer une attente à son travail.
Une notification envoyée à un agent terminé n'est pas une nouvelle tâche active :
le relancer explicitement, puis vérifier son état. Noter les délais d'orchestration
séparément de son analyse ou de son implémentation. De même, un outil qui rend un
handle encore actif n'a pas fini de produire ses fichiers ; attendre sa fin avant
de lancer une étape qui dépend de ces fichiers.

Une revue vérifie le code et les résultats, puis demande : « Pourquoi la variante
suivante n'exigera-t-elle pas une nouvelle branche ? » Documenter une limite réelle
reste acceptable ; enlever arbitrairement une variante légale pour faire passer
les tests ne démontre pas la généricité.

Rapporter séparément : succès du parseur, contrats exécutables, comportements
prouvés, couverture des capacités et cartes complètes, objets jouables/jetons,
combinaisons ajoutées sans code, dette spécialisée et temps de chaque phase.
Les chiffres de complétude seuls ne prouvent pas la composition.

## Mise en place progressive

La prochaine preuve d'architecture, après réparation de l'intégration en cours,
est une verticale `look → choose → move` avec variations indépendantes de quantité,
prédicat et destination, sous les contextes de capacité pertinents. Conserver les
primitives et preuves correctes existantes ; remplacer les parents-recettes au
fur et à mesure que le chemin partagé les couvre.

Construire ensuite une matrice exécutable de capacités à partir des contrats et
fixtures, avec version du compilateur/moteur, consommateurs, combinaisons prouvées,
caps et preuves. Distinguer déclaration de support et preuve actuelle ; éviter un
second inventaire manuel des fonctions. Cette matrice, le classement automatique
des diagnostics et le paquet de langage chargeable sont des chantiers à réaliser,
pas des automatismes annoncés comme déjà disponibles.

Les consignes et les relectures orientent le travail ; les tests de composition,
la liaison à la vraie IR et les régressions en démontrent le résultat. Mesurer le
gain de cette méthode sur les prochains lots et ajuster les règles si la quantité
de code nécessaire par combinaison ne diminue pas.

Le contrat cible des sous-systèmes de règles activables par partie est décrit
dans [oracle-runtime-capabilities.md](oracle-runtime-capabilities.md). Distinguer
compilation Rust, compilation Oracle et exécution ; le manifeste de capacités
doit être dérivé du programme vérifié et couvrir les introductions dynamiques.
Ce découpage est une cible à mettre en place et mesurer, pas un gain déjà obtenu.
