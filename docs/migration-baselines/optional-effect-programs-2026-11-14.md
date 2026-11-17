# Programme d'effets optionnels : checkpoint B1EF accepté

Lot certifié dans `.tmp/night-optional-certified-root-01/certificate.json`.
Analyseur SHA `B1EF3BDD94B2277F002B23F50B8FBABA337B0D417FCFB05F5238B42833CBD0ED`.
Le checkpoint précédent 90A8 et tous les candidats intermédiaires sont conservés.

Le parent syntaxique d'un déclencheur compose maintenant un `EffectProgramNode` :
séquence ordinaire ou permission contenant une autre séquence. Les enfants
gardent les mêmes cibles, nombres, conditions et actions primitives. Le lowerer
place les décisions de ciblage dans la déclaration et les actions dans
`optionalEffects`. Il n'ajoute aucun opcode combiné ni traitement par nom de carte.
Une permission suivie d'une autre phrase, d'un `if you do` ou d'une seconde
permission indépendante reste explicitement refusée tant que sa portée et ses
liaisons de résultat ne sont pas implémentées. Les anciennes séquences typées
complètement reconnues gardent leur IR et leur portée.

Le dispatch reçoit également un résultat typé distinguant absence de candidat,
candidat rejeté, échec engagé et succès. Un curseur avancé ne suffit pas à
revendiquer une phrase. Un succès complet d'un chemin natif existant reste
prioritaire ; sinon l'échec du parent engagé est conservé. Une preuve secondaire
typée conserve aussi les diagnostics des candidats
spécialisés, sans les présenter comme propriétaires de la phrase. Les huit
comparaisons Native/Full gardent exactement les champs des quatre anciens
diagnostics spécialisés, avec seulement leur code de rôle distinct. La trace
secondaire est déplacée par ownership, sans copie inutile.

Mesure finale B1EF : **3 623 cartes non vides complètes et 17 308
capacités natives**, soit +47/+75. Les 17 233 anciennes IR natives sont exactes ;
aucune régression dans le corpus fixe de 39 288 objets et 67 417 capacités.
Les 3 144 objets canoniques sans capacité restent séparés ; un objet sans
capacité est rejeté à la racine et ne compte pas comme succès vide.
Les dix sets restent à 287 occurrences complètes et 1 525 capacités natives
(239 cartes complètes uniques). La cohorte03 garde ses 100 cartes non résolues.
Ces chiffres décrivent la reconnaissance ; ils ne remplacent pas une preuve
de simulation individuelle pour chacune des cartes nouvellement reconnues.

La liaison réelle de Reclamation Sage compare son entrée complète, son texte
Oracle et l'IR émise avec les contrats et fixtures moteur. Le digest officiel
est `BABC2FC58A2490E4829C054864CEE14FA4903F1209EE2042FE6618AD59D7D4C2`.
Voir `.tmp/night-optional-real-link-01/linked-final/proof-links.json` et la
comparaison globale finale qui conserve aussi toutes les IR des candidats.

Le test moteur dédié a trouvé un défaut antérieur : sans remplacement de quantité,
le checkpoint était évité et une erreur de décision optionnelle pouvait laisser
le déclencheur retiré de la pile. Le préflight des permissions réutilise maintenant
le checkpoint complet existant. Sa revue a également relevé que la préparation
des cibles peut supprimer l'ancien sommet ou créer des déclencheurs ; l'analyse
conservatrice doit donc couvrir la pile et cette préparation avant mutation.
Le chemin déterministe sans permission ni déclaration conserve l'absence de clone
inconditionnel. La restauration concerne le moteur, pas l'état externe du fournisseur.

Mesure parser sur les mêmes 2 368 entrées des dix sets, un échauffement par binaire
puis cinq paires alternées : médiane interne finale **0,595 s avant / 0,612 s après**,
processus avec I/O **0,713 s / 0,732 s**. Le feedback détaillé ajoute environ
17 ms (+2,86 %) sur ce lot ; aucun gain de vitesse n'est revendiqué.
Ce n'est pas un benchmark de simulation. Compilation et
diagnostic restent mesurés séparément dans le tracker root et les rapports.

Gates finaux : 885 unités parser, 515 DATA Native et 515 Full, 181 unités moteur,
663 DATA moteur/4 524 assertions, 375 contrats permanents. Les 47 gains passent
également 48 contrats full-input (44 normal, deux host, une transform à deux faces),
sans compilation supplémentaire : exécutabilité statique, pas 47 comportements
individuels simulés. Les 145 historiques strictes restent vertes ; leurs 395
preuves ont été réexécutées après le changement moteur, puis validées en cache
sur la source finale. Sage passe trois preuves fraîches, puis leur validation
finale en cache. Registre suivi : 158 cartes. Architecture : zéro violation,
zéro warning, zéro opcode composite.

La passe large de 884 unités avait révélé douze échecs : une attente de cardinalité
changée par le diagnostic secondaire, neuf attentes plus anciennes incorrectes,
et deux vrais défauts de positionnement du verbe. Ces deux readers sont corrigés
en conservant leurs assertions originales ; les autres tests gardent ou renforcent
leur valeur sémantique. Une unité Kindred supplémentaire donne 885 tests verts.
Le rapport ne prétend pas qu'une ancienne passe large était verte.
Une dette préexistante reste signalée dans la revue : certains chemins compatibles
acceptent une capacité ETB synthétique sur un type Kindred non permanent.

Artefacts intermédiaires : `.tmp/night-optional-root-01/` (comparaisons globale et
dix sets, unités, benchmark), `.tmp/night-optional-fixture-review-01/` (revue
individuelle des 23 écarts, cinq nouveaux succès et dix-huit refus conservés),
`.tmp/night-optional-rollback-review-01/` (contre-exemple de changement de sommet).
Timing : `.tmp/card-migration-timings/night-reclamation-optional-root-01.json` ;
les temps d'analyse/implémentation initiaux de l'agent restent dans son paquet,
distincts de l'intégration root et des attentes pendant les travaux parallèles.
Les six builds du harnais Rust moteur représentent 382,265 s de compilation
contre 9,188 s d'exécution des tests. Les deux premières corrections du harnais
seules coûtent 88,155 s de compilation ; les builds nécessaires aux corrections
moteur ne sont pas présentés comme supprimables. Le banc DATA reçoit donc un
chantier distinct d'injection d'erreur et de reprise, pour faire ces prochains
diagnostics sans relinker un test Rust par scénario.
