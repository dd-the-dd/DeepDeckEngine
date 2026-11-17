# Audit local et entretien du cache incrémental

Le mode `local-audit` du runner conserve le même artefact Oracle que le runner
de fixtures, sans dépendance au moteur. `audit-cli` ajoute le téléchargement
distant et reste compatible ; aucun appel réseau n'a été effectué pendant cette
validation. Supprimer seulement une feature Oracle n'avait pas suffi : les
dépendances réseau changeaient aussi les macros Serde et reconstruisaient Oracle.

Preuve réelle : `.tmp/night-local-audit-root-01/artifact-reuse-proof.json`.
Les deux builds release réutilisent le même `libdeepdeck_oracle-771e2fbac41d1ba4.rlib`
(`fresh=true`), avec les features `default, parser-standalone`, zéro artefact moteur
et un Cargo.lock inchangé. Préparation/build des binaires : 3,87 s puis 5,06 s.
Le binaire avec `audit-cli` compile également (5,46 s). Le binaire local refuse
explicitement une entrée `set:` avec le code 2.

La régression stricte conserve 145/145 cartes et 244 capacités natives :
`.tmp/night-local-audit-root-01/strict145-root.json`. Audit parser : 0,121 s ;
préparation Cargo : 2,128 s ; total avec architecture, entrées et orchestration :
15,504 s. Les 395 preuves déclarées restent valides en cache ; aucune n'est
présentée comme réexécutée dans cette passe. Les tests AST/cache et UTF-8 passent.
Les 500 cas parser passent aussi dans chacun des modes NativeOnly et Full,
sans fixture invalide (rapports `parser500-*-root.json` dans le même dossier).

L'entretien incrémental est désormais sérialisé avec Cargo et mesuré séparément.
Il sélectionne des groupes complets anciens, protège les variantes récentes et
les groupes actifs, refuse les liens et revalide les chemins avant suppression.
Il ne supprime ni `deps`, ni sources, ni fixtures. Si les fichiers hors cache
incrémental rendent le budget inatteignable, il le signale plutôt que de vider
continuellement tout le cache.

Validation : 27 contrôles synthétiques, six contrôles des ancêtres et cinq
régressions Int64 permanentes, sans grosse allocation ni suppression réelle.
Un premier dry-run a révélé une sélection incorrecte de surcharge `Math.Max`
au-delà de 2 Gio ; les deux appels utilisent maintenant explicitement Int64.
Le dry-run corrigé relève 38 396 919 612 octets et ne prévoit aucune suppression.

Mesure chaude réelle : `.tmp/night-cache-root-01/warm-wrapper-root.json`.
Quatre tests passent ; entretien 1,755 s, vérification Cargo sans reconstruction
0,298 s, exécution 0,050 s, total 2,157 s. Cette mesure ne démontre pas une
accélération des simulations ni un gain d'éviction : aucun fichier n'a été retiré.

Ces changements améliorent la boucle de travail ; ils n'ajoutent aucune carte
au checkpoint sémantique 90A8 (3 576 cartes non vides / 17 233 capacités natives).
