# Erreurs de décision et reprise dans le banc DATA

Extension acceptée : `.tmp/night-runtime-data-failure-root-01/certificate.json`.
Le moteur, le parser Oracle et le lockfile sont inchangés par ce lot.

`probeFailure` exécute une action publique réelle, injecte une erreur au callback
demandé, vérifie l'erreur propagée et l'égalité de l'état public avant/après.
Le pilote ne restaure jamais le moteur pour faire passer le test. La reprise
rejoue les réponses et compare état, demandes et compteurs avec un clone propre.
Les cinq callbacks faillibles sont couverts ; les gardes méthode/joueur/requête
et les erreurs naturelles avant l'injection ne peuvent pas créer un faux succès.
`executeRule` expose aussi l'API d'exécution existante dans la séquence DATA.

Validation : 12 unités de bibliothèque, dont huit nouvelles ; 666 fixtures et
4 547 assertions passent. Les 663 anciens résultats sont structurellement exacts,
types et casse compris. Les trois nouvelles fixtures ajoutent 23 assertions :
programme direct, cibles puis permission, et changement du sommet avant permission.

Après le build du pilote et de ses tests (21,87 s et 17,05 s, mesurés séparément),
les trois scénarios se réexécutent en **0,046 s, zéro Cargo**. Il ne s'agit pas
d'une mesure de vitesse du moteur ni d'une économie déjà réalisée sur les prochains
lots. La revue complète a précédé ce build ; aucun correctif de production n'a
été nécessaire après compilation.

Le wrapper chronométré accepte maintenant
`-RuntimeTestTarget deepdeck-runtime-fixture-runner -RuntimeTest '_'` pour cette
bibliothèque, sans activer les anciens tests moteur dépendants d'Oracle.

Les DATA observent l'état public et les conséquences d'une reprise. Le test
architectural privé des RNG, frames et compteurs reste nécessaire. Un refus
interne avant commit ne prouve pas le rollback du résolveur public : ce pilote
doit justement détecter un élément de pile perdu ou des effets antérieurs conservés.
