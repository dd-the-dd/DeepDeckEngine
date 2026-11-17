# Itération Player et séparation des départs : phase1 acceptée

Le moteur dispose maintenant d'une vraie boucle sur les joueurs : corps
composé d'effets ordinaires, liaison de joueur locale, ordre APNAP et collecte
explicite des objets à exporter. Les environnements locaux sont restaurés
sur erreur ; aucun snapshot universel de GameState n'est ajouté. Un joueur
éliminé par une itération précédente est ignoré lorsque son tour arrive.

Le nouveau domaine battlefield_departure possède la planification commune
de destination et sépare l'installation du changement de zone de sa
publication. Destruction conserve ses préventions propres ; les wrappers
unitaires conservent leur ordre précédent. Cette extraction ne prétend pas
encore rendre tous les changements de zone simultanés. Le vieux template
forEachPlayer reste en place jusqu'au paquet de sacrifice simultané.

Root a revu et appliqué les paquets36771, D3CE et l'addendum21C123. Sept
chemins initiaux, puis deux fichiers modifiés par l'addendum. Dans mod.rs,
seules quatre fonctions changent : effect_supported, resolve_players,
execute_effect et exile_permanent. L'ordre des modules est conservé.

Preuves : 109 tests privés moteur ; 594 scénarios DATA /4 000 assertions ;
532 contrats ; préservation de 135 groupes /16 656 capacités, plus les
48 et120 gains précédents ; zéro violation d'architecture. Le préfixe
strict passe145/145 avec395 preuves fraîches, sans réseau. Aucun gain de
couverture n'est revendiqué : le parser D648 et ses audits restent exacts,
soit16 824 capacités reconnues et3 374 objets non vides entièrement reconnus.

Deux défauts ont été conservés dans les preuves initiales : le témoin du
joueur éliminé échoue avant l'addendum puis passe après ; une nouvelle DATA
utilisait object au lieu de permanent dans putCounters. Seule cette clé a
été corrigée, sans modifier les attentes. Trois nouvelles DATA et six
contrats ont été promus dans les dossiers de tests dédiés.

Temps : premier build privé65,342 s /exécution1,768 s ; après correctif,
44,434 s /1,681 s. Runner après correctif13,650 s /0,599 s pour le témoin.
Le gate permanent sans compilation prend2,499 s pour594 scénarios. Le gate
strict complet prend25,764 s. Ces mesures ne prouvent pas une accélération
du build par rapport au dépôt ancien : les entrées de compilation diffèrent.
L'intervalle root588,674 s comprend revue, orchestration et attentes ; voir
PHASE1-TIMING-SNAPSHOT.json dans .tmp/native-player-departure-root/.

Le sacrifice simultané, le retrait du template et la dette du retour
Earthbend immédiat restent séparés. Le certificat de cette phase ne valide
aucun paquet encore en staging. Objectif global toujours actif.
