# Couleurs distinctes et destruction générique

Bloom Tender est vérifiée de bout en bout : le registre passe de173à174cartes.
La capacité mana de Faeburrow Elder est reconnue ; son bonus continu reste à
implémenter. Le rejeu de39288cartes donne3900complètes non vides et17964capacités
natives : +1carte complète,+2capacités. Les17961autres anciennes règles restent
exactes, avec une réécriture prévue de Wrath of the Skies.

Wrath utilise désormais la primitive de destruction existante, une requête de
permanents et une comparaison de valeur de mana. La préparation des nombres a
lieu après le paiement, avant la sélection et la destruction simultanée. Un
résultat manquant produit une erreur avec restauration de l’état. Types effectifs,
phasing, protections et informations de dernier état connu sont testés.

22anciens effets spécialisés retirés ; toutes les primitives génériques restent.
Quatre compositions natives restent à décomposer. Les compteurs246acceptés et
247exécutés comptent des noms d’opérations, pas des primitives atomiques.

| Contrôle | Résultat |
|---|---:|
| Tests unitaires parseur |969|
| Cas Native / Full |702dans chaque mode|
| Scénarios moteur |829 /5616assertions|
| Contrats |427|
| Tests moteur CI3 |363|
| Tests Wrath FSM |6|
| Preuves strictes Bloom/Wrath |8fraîches|
| Préfixe historique |145cartes /395preuves fraîches|

Les dix sets de référence passent de307à308occurrences complètes et de1584à1585
capacités natives, grâce à Bloom Tender dans ECL (27→28complètes,136→137capacités).
Toutes les autres lignes sont inchangées.2368occurrences=308complètes+927partielles+
1130nonprisesencharge+3vides ;2319identités uniques,260complètes.

Compilation/exécution ciblées : parseur15,157/0,360s ; moteur31,663/0,569s.
Les829scénarios complets s’exécutent en4,044s sans compilation. Les13audits prennent
12,148s. Les phases d’analyse/intégration des agents se recouvrent ; aucun temps
pur de développement n’est reconstitué et aucun gain général de vitesse n’est revendiqué.

**Défaut découvert après ce checkpoint :** une capacité mana avec sacrifice en
coût peut utiliser les couleurs d’avant paiement. Le runner courant a reproduit
le sacrifice correct mais une mana verte indue. Ce cas voisin n’est pas couvert
par les preuves de Bloom à coût d’engagement. La correction générique du calcul
après coûts, avec planification, choix, dernier état connu et restauration, est
en cours ; le test de comportement est conservé. La migration globale reste active.

Certificat837sources : `.tmp/night-color-query-destruction-root-01/certificate.json`,
SHA2568558F1AA296562B89C31D9AE88DC169EEE972E84B206D6AA62C44CCBF1DC7AB5.
Complément obligatoire : `postcost-erratum.json` dans le même dossier, avec preuve
sur le runner courant dans `.tmp/night-mana-postcost-independent-review-01`.
