# Cinq formes moteur sans producteur retirées

Les formes putLandFromHandIfTriggeringPlayerAhead,
grantSingleCastPermissionIfNoSpellCast, preventNextDamageAndGainLife,
grantWebSlinging et grantMayhem sont retirées. Elles regroupaient des
concepts indépendants et n'avaient plus de producteur actuel. L'inventaire
épingle1 949 fichiers et les16 862 IR natives réelles, dont les38 gains
derniers. Les permissions, les vrais keywords et leurs primitives restent
conservés, ainsi que les consumers de modes dérivés non inclus dans ce lot.

Deux fichiers changent : mod.rs et la garde d'architecture.223 lignes moteur
nettes retirées ; cinq fonctions modifiées, aucun helper ni test privé
supprimé. Les fonctions et dépendances non concernées restent byte-exactes.
La garde contient maintenant113 kinds retirés. Ce n'est pas une certification
de pureté exhaustive de tout le moteur.

Root a corrigé avant toute application un retrait de mut incorrect dans le
draft fused. Le patch final89507 conserve les affectations split et fused.
Le draft918AF reste archivé, sans aucune compilation ni écriture production.
Le vieux payload généré Webslinging était déjà refusé au contrat ; cette dette
n'a pas été élargie ni masquée pour ce nettoyage.

Preuves fraîches :610 scénarios moteur /4 119 assertions,580 contrats,
135 groupes historiques,145 cartes strictes /395 preuves et zéro violation
d'architecture. Cinq refus explicites et deux contrôles distincts sont promus
dans des JSON dédiés. Trois autres contrôles passent dans la gate transitoire
583, puis restent hors gate permanente : deux doublons sémantiques et un
diagnostic de payload legacy déjà invalide. Aucun nouveau harness Rust.

Le parser et son analyseur F096 sont inchangés : les preuves de reconnaissance
sont héritées à hashes exacts, soit16 862 capacités et3 403 objets non vides
entièrement reconnus. Aucun gain de couverture supplémentaire n'est revendiqué.
Les109 privés runtime restent hérités du certificat BA147, pas fraîchement
exécutés sur64354. Le nouveau runtime610 est compilé sur64354.

Mesures : runner moteur23,403 s de compilation /4,743 s d'exécution ; liaison
contrat1,581 s et gate finale0,222 s ; strict25,985 s. L'intervalle root
305,929 s inclut revue et orchestration. Ces mesures ne démontrent pas un
gain de simulation attribuable au retrait. Journal :
`.tmp/card-migration-timings/native-five-dead-forms-integration.json`.

Legacy conservé sur la branche distante archive/legacy-oracle-2026-11-13,
commit eec9a2ef4a57b7a759e0b0cd667cdae70f1a0c99. Aucun commit de nettoyage
ni mutation d'index. Le sacrifice simultané, l'ancien template Player et les
autres dettes explicitement documentées restent un chantier séparé.
