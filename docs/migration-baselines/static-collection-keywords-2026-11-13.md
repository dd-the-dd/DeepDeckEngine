# Mots-clés statiques des collections — 13 novembre 2026

Le nouvel automate compose les enfants ObjectQuery et KeywordList. Il distingue les alternatives de types des mots-clés accordés simultanément, conserve les scopes et exige une enveloppe permanente complète. Conditions, durées, phrases supplémentaires et connecteurs incorrects sont refusés explicitement. Le moteur reçoit un grantKeyword atomique par mot-clé, actif pendant la présence de la source sur le champ de bataille. Aucun changement Rust du moteur.

| Même corpus : 39 288 objets /67 420 capacités | Après Source | Après Temur | Delta |
| --- | ---: | ---: | ---: |
| Capacités natives reconnues | 16 575 | 16 656 | +81 |
| Objets non vides entièrement reconnus | 3 275 | 3 297 | +22 |
| Lot de 100 : objets entiers | 1 | 2 | +1 |
| File de 450 : objets entiers | 111 | 112 | +1 |

Les 16 575 anciennes IR sont strictement préservées. Les 81 nouvelles règles passent le contrat exécutable, zéro fixture invalide. Les API par défaut/native sont identiques. Depuis le retrait C355 : +623 capacités et +328 objets entiers ; les ensembles de nouveaux objets entiers des deux généralisations sont disjoints. Les 3 144 objets sans texte sont séparés. Ces comptes de reconnaissance et de forme ne sont pas des simulations par carte.

Preuves fraîches : 320/320 cas parser, 5/5 tests privés ciblés, 6/6 contrats dédiés, 9/9 scénarios moteur /64 assertions. La règle complète de Temur comporte ses deux capacités, structurellement égales aux règles émises. Deux variantes de données font entrer une créature de force4, vérifient simultanément haste et la pioche facultative acceptée/refusée ; le cas de force2 ne déclenche pas la pioche. Sont aussi testés sortie de la source, source en main, contrôleur différent du propriétaire et autres collections.

Préfixe indépendant : 145/145 via145 preuves en cache. Un audit supplémentaire des deux vraies cartes Solemn et Temur, avec le manifeste permanent, exécute17preuves et passe2/2, sans build de binaire et sans fetch. Architecture globale sans violation. Les16 anciens rouges de la suite privée élargie au nettoyage restent documentés ; cette suite n'est pas présentée verte.

Temps : privé Rust14,146s de compilation et1,017s d'exécution ; runner stable11,346s et0,796s pour320cas ; release32,420s ; global natif8,065s ; moteur neufcas0,107s sans recompilation ;81contrats de gains0,034s. Le journal `.tmp/card-migration-timings/native-temur-postcleanup-integration.json` et les rapports bruts font autorité ; le temps mural de coordination n'est pas un temps CPU par carte.

Les quatre anciens refus contractuels restent inchangés. L'audit supplémentaire du moteur a aussi identifié amass et explore comme opérations composées encore accessibles (notamment Bowmasters et le jeton Map) : elles doivent être migrées avant retrait. Le guard sans violation ne prouve pas à lui seul que chaque opération restante est primitive. Aucune suppression aveugle de ces producteurs.

L'archive distante du legacy reste `archive/legacy-oracle-2026-11-13`. Les sources de travail sont non commitées et l'index initial reste inchangé. La mission globale continue avec les remplacements de quantités et le prochain lot retenu/rempli par le script officiel.
