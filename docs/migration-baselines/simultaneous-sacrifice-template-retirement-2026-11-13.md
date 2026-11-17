# Sacrifice simultané et retrait du template Player

Le moteur prépare les destinations et permissions avant les départs, installe
tous les mouvements, puis enregistre l'événement complet avant les callbacks.
Les choix des joueurs sont APNAP et précèdent le sacrifice collectif. Le corps
de la boucle Player ne connaît ni groupes de types ni sacrifice particulier :
les choix, collections et différences d'ensembles sont des effets composables.

Les deux anciens arms forEachPlayer sont retirés :176 lignes /8 790 octets.
Le même kind reste une vraie primitive d'itération avec bind explicite.
Le patch principal0B217 modifie12 chemins, dont deux DATA historiques dont
seuls les effets changent. Leurs setups et attentes restent identiques. La
composition sur643 conserve les cinq suppressions précédentes et leur garde113.

La sélection d'objets sur le champ de bataille réutilise les caractéristiques
effectives et le moteur commun de prédicats/nombres ; les autres zones gardent
leur vue de définition. Les coûts et effets partagent la permission de sacrifice
mais conservent leurs origines distinctes. Les préventions de destruction ne
préviennent pas le sacrifice ; un remplacement de destination peut l'envoyer
en exil sans produire de mort. Les sources d'observation sont capturées avant
les départs. Les cartes entrant au cimetière sont regroupées par propriétaire,
avec tous les records disponibles avant les conditions de déclenchement.

Preuves fraîches :123 privés moteur,618 scénarios DATA /4 166 assertions,
587 contrats,135 groupes historiques et145/145 strictes /395 preuves.
Architecture : zéro violation. Huit DATA et sept contrats synthétiques sont
promus dans des fichiers dédiés. Aucun gain de carte n'est attribué à ce lot.
Le parser F096 est inchangé à hashes exacts ; score hérité16 862 capacités
et3 403 objets non vides entièrement reconnus.

La première passe privée a119 verts et4 rouges. Le correctif7A6 est uniquement
du setup : synchroniser printed_definition, fournir les players exigés par
deux watchers GY et utiliser les comptes globaux sans filtre contrôleur.
Les76 macros d'assertion et14 tests sont inchangés ; quatre contrats de feuilles
ont été validés avant recompilation. Aucun comportement moteur n'a été changé
pour rendre ces tests verts.

Temps : premier privé68,476 s de compilation /1,819 s d'exécution ; correction
test seule7,399 s /1,616 s. Premier runner616 :26,755 s /3,286 s. Après le
setup, Cargo ne fait qu'une vérification de fraîcheur0,198 s et618 DATA prennent
2,711 s ; le gate permanent sans build prend2,644 s. Strict :26,531 s.
Le journal partagé a subi une erreur d'ownership sans perte d'historique : un
agent y a fermé la phase root puis ouvert sa phase suivante. L'intervalle est
explicitement marqué et le nouveau travail utilise un ID distinct. Les mesures
de référence sont dédupliquées par fichier/hash dans CANONICAL-TIMING-REPORTS.json ;
les événements répétés du journal ne doivent pas être additionnés.

Limites explicites : le LKI démontré concerne les aptitudes déjà en pile avant
le départ. Deux probes actuels hors gate montrent encore une mauvaise puissance
pour une nouvelle aptitude self-dies après le départ d'un lord et un doubleur
de déclenchement disparu trop tôt. Leur comportement antérieur n'a pas été
mesuré. Un troisième probe montre qu'un filtre Creature de sacrifice lit encore
la définition imprimée d'un terrain animé. Le futur parser refuse donc les
filtres non nus jusqu'à une capture commune des matches effectifs avant commit.
Ces probes sont conservés ; aucune pureté universelle LKI n'est revendiquée.

Le retour Earthbend immédiat reste une dette distincte de son déclencheur
différé réglementaire. L'addendum de scope sacrifice D25 n'est pas inclus.
Les correctifs Battle/zéro dégâts et le parser Mayhem sont encore en staging.
L'objectif global de migration reste actif.
