# Déclencheurs spellCast : programme partagé accepté

La lecture des déclencheurs de lancement de sort utilise un seul automate
d'événement. Les parents composent son filtre, son joueur et son éventuel
ordinal avec la séquence d'effets existante. Les anciennes vues fiscales
conservent leurs IR ; aucun opcode propre à une carte n'est ajouté.

Sur les mêmes 39 288 objets locaux / 67 420 capacités : **16 824 capacités
natives** et **3 374 objets non vides entièrement reconnus**, soit **+120 et
+59**. Les 16 704 anciennes capacités natives gardent exactement leur IR.
Les 3 144 objets sans texte sont séparés. Les deux API produisent un audit
identique. Ces nombres mesurent la reconnaissance ; ils ne constituent pas
une preuve de simulation individuelle de chaque carte.

Les 120 nouvelles formes passent le contrat moteur. Beast Whisperer et
Lotho passent leurs vrais Oracle complets, leurs contrats et neuf scénarios
d'exécution : onze preuves verticales fraîches. Le manifeste permanent
contient maintenant 152 entrées. Le lot18 passe de 29 à 31 capacités natives
et de zéro à deux cartes entières ; la queue450 compte 118 cartes entières.

Régressions : 369 cas parser distincts dans chaque mode, 591 scénarios
moteur / 3 982 assertions, 96 tests privés moteur, 526 contrats et le
préfixe strict 145/145 avec 395 preuves fraîches. Les 372 cas parser bruts
incluaient trois doublons exacts retirés lors de la promotion. Architecture :
zéro violation. Le corpus historique de 135 groupes préserve 16 656 formes,
les 48 gains précédents étant aussi rejoués dans les contrats.

Temps mesurés séparément : compilation initiale du runner moteur 23,614 s,
exécution 3,134 s ; compilation privée moteur 54,327 s, exécution 1,737 s.
Les relectures sans compilation prennent 2,786 s pour le moteur et 0,594 s
pour le parser natif. L'audit optimisé des 67 420 capacités prend 7,982 s
(7,941 s par l'API par défaut). Le gate vertical deux cartes prend 10,195 s,
sans compilation ni réseau. Les durées root comprennent revue, corrections
et orchestration ; elles ne sont pas du temps CPU par carte. Voir le journal
`.tmp/card-migration-timings/native-spell-cast-event-integration.json`.

Les premières compilations et corrections restent conservées : convergence
des lecteurs d'événement, vue ordinale nue, frontière de claim fiscal,
diagnostic négatif Esper et configuration lexicale d'un helper privé. Un
échec de test n'a pas été effacé pour obtenir un résultat vert.

Limites : les comparaisons numériques portant sur un sort observé restent
refusées dans le nouveau parent. Le matcher historique lit encore la valeur
de mana imprimée, sans le X choisi sur la pile ; les sondes correspondantes
restent hors corpus. L'ordinal de sous-ensemble filtré n'est pas ajouté à ce
parent. Le template forEachPlayer et la simultanéité des sacrifices sont
encore en chantier séparé. Les seize échecs privés parser connus au nettoyage
ne sont pas déclarés corrigés. La garde des 108 kinds retirés ne prouve pas
à elle seule que toute la sémantique du moteur est primitive.

Le legacy reste archivé sur `archive/legacy-oracle-2026-11-13`, commit
`eec9a2ef4a57b7a759e0b0cd667cdae70f1a0c99`. Aucun commit de nettoyage,
aucune mutation de l'index initial. Les travaux clock/training/backend restent
préservés. Certificat : `.tmp/oracle-checkpoint-spell-cast-D648FA02.json`.
