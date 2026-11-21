# Programme de déclenchement de la source — 13 novembre 2026

Après le retrait du legacy, l'automate reconnaît une entrée ou une mort de la source, puis délègue le corps au séquenceur partagé. Les choix facultatifs de recherche et de pioche composent les primitives existantes. Aucun opcode combiné ni fonction par carte n'est ajouté au moteur.

Le sous-automate restitue position et état au parent quand il ne revendique pas la famille ; la trace conserve son préfixe et un marqueur de rollback. Une erreur après engagement garde son diagnostic. Les entiers respectent la représentation i32 du runtime ; les bindings X sont validés dans la bonne face. Une pioche facultative exacte et une pioche « jusqu'à N » conservent leurs choix différents.

| Même corpus local | Avant C355 | Après 0BAB | Delta |
| --- | ---: | ---: | ---: |
| Capacités reconnues natives | 16 033 | 16 575 | +542 |
| Objets non vides entièrement reconnus | 2 969 | 3 275 | +306 |
| Objets sans texte, comptés séparément | 3 144 | 3 144 | 0 |
| Lot de 100 : capacités natives | 32 | 34 | +2 |
| Lot de 100 : objets entiers | 0 | 1 | +1 |
| File de 450 : objets entiers | 109 | 111 | +2 |

Les 39 288 objets et leurs 67 420 capacités sont identiques et dans le même ordre. Les 16 033 anciennes IR natives sont structurellement identiques, casse comprise, sans normalisation. Les API native et par défaut produisent les mêmes sorties. Les 542 nouvelles règles passent le contrat de forme exécutable. Les quatre anciens refus de forme restent une dette distincte : ce test ne remplace ni la composition de toutes les faces ni des scénarios de partie pour chacune des 306 cartes.

Solemn Simulacrum est la verticale prouvée du lot : son Oracle complet authentifié produit les deux règles exactes employées par les scénarios moteur. Les cas couvrent entrée, recherche, échec à trouver, refus, mort, exil sans mort, dernier contrôleur et source ayant quitté le champ. Les variantes synthétiques vérifient les choix exacts/jusqu'à N, y compris 65, sans être comptées comme cartes.

Preuves : 306/306 cas parser, 17/17 tests privés ciblés, 16/16 scénarios moteur et 131 assertions, 8/8 contrats ciblés. Préfixe historique 145/145 avec 145 preuves en cache, aucune nouvelle exécution de ces preuves et aucun fetch. Architecture : zéro violation. La suite privée élargie n'a pas été rejouée ici ; ses 16 échecs connus au checkpoint de nettoyage restent documentés, sans prétendre que toute la suite est verte.

| Mesure | Compilation/liaison | Exécution |
| --- | ---: | ---: |
| Premier runner parser après source | 12,412 s | 0,708 s /306 cas |
| Rejeu permanent après dernier test privé | 0,248 s, Cargo à jour | 0,497 s /306 cas |
| Parser optimisé | 35,108 s | 7,898 s /corpus global natif |
| Scénarios moteur Solemn | 0 s | 0,129 s /16 cas |
| Contrats ciblés | 0 s | 0,058 s /8 cas |

Les temps muraux d'intégration comprennent revue, coordination et audits ; ils ne sont pas du temps CPU par carte. Le journal `.tmp/card-migration-timings/native-solemn-postcleanup-integration.json` conserve les commandes et une boucle corrective du test de trace. Les mesures de développement initiales de l'agent sont conservées séparément. Aucun gain de simulation de partie n'est revendiqué.

La migration continue. Le lot exact reste gelé pour comparer les généralisations Temur et quantités en préparation ; il sera ensuite complété par le script officiel, sans perdre les cartes non couvertes ni leurs provenances.
