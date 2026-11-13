# Delta NativeOnly des 10 derniers sets — CFCC4C9F

> **Checkpoint non promouvable en Full.** La reconnaissance NativeOnly est strictement sans perte par rapport à B13, mais un probe Full ciblé montre 12/12 interceptions régressives d'anciennes capacités legacy canoniques. Les chiffres ci-dessous mesurent donc le gain du parseur natif, pas un état de release compatible.

## Delta B13 → CFCC4C9F

| Set | Capacités natives | Delta | Complètes avec capacité | Delta | Partielles | Aucune native |
|---|---:|---:|---:|---:|---:|---:|
| MBC | 26 / 181 | 0 | 8 | 0 | 15 | 57 |
| FRA | 178 / 562 | +2 | 33 | 0 | 109 | 141 |
| HOB | 103 / 387 | +3 | 10 | 0 | 81 | 101 |
| MSH | 193 / 590 | +1 | 16 | 0 | 141 | 124 |
| SOS | 173 / 506 | +1 | 32 | +1 | 99 | 140 |
| TMT | 106 / 408 | +1 | 14 | 0 | 79 | 102 |
| ECL | 120 / 540 | 0 | 20 | 0 | 80 | 173 |
| TLA | 169 / 604 | 0 | 30 | 0 | 94 | 162 |
| SPM | 113 / 400 | 0 | 13 | 0 | 86 | 94 |
| EOE | 123 / 519 | +1 | 20 | +1 | 81 | 165 |

Sur les occurrences par set, CFCC atteint 196 cartes complètes avec capacité, 3 vanilles, 865 partielles et 1 259 sans capacité native, pour 1 304/4 697 capacités natives. Le delta est donc **+9 capacités natives** et **+2 cartes complètes**. Les 1 295 anciennes capacités natives de ces dix sets sont préservées.

Après déduplication intersets, CFCC atteint 153 cartes complètes avec capacité + 3 vanilles, 864 partielles et 1 259 sans capacité native. Cela représente **156/2 279 cartes complètes (6,85 %)** et **1 260/4 652 capacités natives (27,09 %)**, soit +2 cartes complètes et +9 capacités depuis B13.

## Contrôles globaux

- Round 11 : +2 capacités, +1 carte complète, 0 régression NativeOnly.
- Queue 450 : +3 capacités, +1 carte complète, 0 régression NativeOnly.
- Sept sets locaux : +7 capacités, +4 cartes complètes, 0 régression NativeOnly.
- Global 39 288 : +170 capacités, +40 cartes complètes; 14 751/14 751 capacités antérieurement natives préservées.
- Contrats générés : 210/210 verts.

## Blocage Full identifié

Le nouvel automate revendique 970 capacités auparavant non natives; seules 169 de ces revendications deviennent canoniques dans le set de claims analysé. Sur un probe de 12 anciennes capacités `canonical/legacy`, les 12 deviennent `unsupported/native` en Full CFCC. Les familles confirmées incluent :

- rappel après `surveil`;
- sujet `each opponent` pour la pioche;
- `amass` avec rappel;
- création de tokens dont l'identité n'est pas cataloguée;
- `sacrifice ... unless you pay`;
- sacrifice d'un sous-type;
- gain de plusieurs mots-clés par une cible;
- token complexe suivi d'une anaphore.

Le problème est une frontière de commit trop précoce après consommation partielle du premier effet. Ce checkpoint ne doit donc pas remplacer B13 comme baseline Full avant correction et nouvelle preuve ciblée.

## Preuves

- Analyseur CFCC : SHA-256 `314292D342BF3BF56992B3CDC52286612F0E1644C164847AA4DF8175D131AC10`.
- Rapport détaillé dix sets : SHA-256 `53466D8F3DDBE7B08E36AB3CDFE091D8F4D949D9E37FAF191A9ECBA637320162`.
- Vérification indépendante dix sets : SHA-256 `B3622D75618071F23A716827F48D0C620BBA98A85E793F043FE36898C4641EB6`.
- Comparaison globale stricte : SHA-256 `62B215BA76A09C5F066E3D27A207AAE35794783448EBCDEB39567288AB32D394`.
- Contrats générés : SHA-256 `A78182F3D4B3E54177681F8BB9930AE6F5D48F157535BC89CFDD9B3F3DFE7E99`.
- Probe Full rouge 12 cartes : SHA-256 `DD00356068FF212B86CA3EB692573F873EB16DAD725EA024B7E09008BD11DC63`.
- Baseline Full B13 ciblée complète, 957 cartes : SHA-256 `FB0DB2FF0DEBE6C3E1698EE41A131630399A96737374D32782A011ED5437BA39`.

