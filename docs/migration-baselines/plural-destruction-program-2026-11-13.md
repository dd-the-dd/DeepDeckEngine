# Destruction de plusieurs cibles : vague acceptée

La même TargetList représente une seule déclaration avec sa cardinalité et
son prédicat. Le parent atomique et EffectSequence autorisent explicitement
la collection de destruction ; les autres parents et actions gardent leur
contrat scalaire. Le lowering conserve chosenTarget pour une cible et émet
chosenTargets pour une collection. Aucun opcode nouveau ni regex ajouté.
Une collection ne devient pas un antécédent singulier. La négation de couleur
réutilise les prédicats logiques partagés avec un domaine permanent positif.

Le runtime revalide chaque cible par le validateur existant, puis appelle
une seule fois le batch de destruction avec cannotRegenerate. Quatre défauts
reproduits avant correctif passent maintenant : cible partiellement blinkée,
protection issue d'une source détruite, remplacement de destination issu
d'une source détruite et interdiction de régénérer. Les préventions et les
remplacements restent des domaines partagés, sans interprète par carte.

Sur les mêmes 39 288 objets locaux /67 420 capacités : **16 862 capacités
natives** et **3 403 objets non vides entièrement reconnus**, soit **+38 et
+29**. Les 16 824 anciennes IR restent exactement préservées. Les deux API
ont des sorties identiques ; les 3 144 objets sans texte restent séparés.
Depuis C355 : +829 capacités et +434 objets entiers. Ces nombres mesurent
la reconnaissance, pas le comportement individuel de toutes les cartes.
Les 38 nouvelles formes passent le contrat moteur.

Force of Vigor, Hex et Reckless Spite ont des preuves verticales complètes :
leurs quatre capacités réellement émises correspondent exactement aux
sources complètes des quatorze DATA réelles, dont un négatif de sélection
noire. Trois contrats et ces quatorze scénarios donnent17 preuves fraîches.
Deux scénarios synthétiques supplémentaires comparent la régénération permise
et interdite. Le lot19 gagne une carte entière ; les deux frères hors lot
ne sont pas ajoutés à son score. Le manifeste permanent contient155 entrées.

Régressions : 380 cas parser dans chaque mode, quatre privés parser,
610 scénarios moteur /4 119 assertions, 573 contrats, 135 groupes historiques
et le préfixe strict145/145 avec395 preuves fraîches. Architecture : zéro
violation. Les109 privés moteur appartiennent au certificat parent BA147 ;
ils ne sont pas présentés comme une nouvelle exécution sur827F. Les seize
rouges privés parser connus au nettoyage ne sont pas déclarés corrigés.

Mesures séparées : parser debug11,570 s de compilation et0,797 s d'exécution ;
privés parser17,392 s et1,095 s ; runner moteur21,284 s et3,252 s pour609 cas.
Après promotion,610 scénarios sans compilation prennent6,164 s. L'audit
global optimisé prend7,533 s natif et7,847 s par défaut ; celui du lot100
prend29 ms. Le release est construit en27,902 s, l'analyseur lié en1,109 s.
Le strict prend59,778 s, dont34,575 s de phase parser-build. Les temps ne
sont pas additionnés aux intervalles root ni aux travaux parallèles.

L'agent parser a mesuré186,017 s d'analyse et562,096 s d'implémentation de
staging. La revue/intégration root dure853,412 s et inclut coordination,
corrections des helpers et attentes de preuves. Son journal fermé est
`.tmp/card-migration-timings/native-plural-destruction-root-integration.json`.

Le premier audit vertical a correctement refusé les trois nouvelles pins
de manifeste : le helper utilisait l'ordre des clés des IR planifiées.
La fonction de digest actuelle conserve cet ordre. Les pins ont été
recalculées depuis les objets réellement émis, après preuve d'égalité
structurelle sensible à la casse ; seules ces trois pins ont changé.
Aucun Oracle, parser ou comportement n'a été modifié pour ce correctif.
Les premières sorties restent conservées. Hex avec cinq cibles prouve
l'indisponibilité d'un lancement, sans prétendre tester un rollback de paiement.

Le sacrifice simultané et le retrait du vieux template Player restent dans
un paquet distinct en staging. Les comparaisons numériques du contexte de
pile et les autres dettes héritées restent explicites. La migration globale
continue ; aucune pureté totale du moteur n'est revendiquée.
