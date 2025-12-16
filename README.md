# FAT32_parsing

Dans ce projet, j’ai réimplémenté une partie de FAT32 en Rust à partir d’une image disque brute comme `disk.img`. L’idée de départ était simple : je voulais pouvoir charger le disque en mémoire, comprendre sa structure “à la main”, puis reconstruire l’accès aux fichiers comme un vrai système de fichiers le ferait. Au final, je peux naviguer dans l’arborescence, lister des répertoires, lire des fichiers, et aussi écrire un fichier dans l’image de façon persistante (donc le contenu reste dans `disk.img` après le programme).

Je me suis volontairement limité à une version claire et contrôlée. Je gère les noms courts FAT (format 8.3), parce que ça permet déjà de faire une implémentation complète et solide sans tomber dans toute la complexité des Long File Names. L’objectif était d’avoir quelque chose de propre et démontrable, pas de couvrir tout le standard.

---

## Comment j’ai travaillé

Avant d’attaquer FAT32, je me suis remis dans le contexte “bas niveau”. Comme je voulais un code utilisable dans un contexte `no_std`, j’ai pris le temps de revoir comment organiser un projet Rust avec une bibliothèque qui n’a pas besoin de `std`, et un petit binaire séparé qui s’occupe seulement de l’I/O et de l’affichage. Ça m’a aussi permis d’utiliser `alloc` correctement, parce que même en `no_std`, on a besoin de `Vec` et de `String` dès qu’on reconstruit des chemins et du contenu de fichier.

Ensuite je me suis concentré sur les trois blocs essentiels de FAT32 : le BPB (le header du volume), la FAT (la table qui chaîne les clusters) et les entrées de répertoire (les structures de 32 octets). À partir de ça, tout devient une histoire d’offsets, de tailles, et de lecture de bytes.

---

## Ce que ça veut dire “parser FAT32 en bytes”

Tout mon projet repose sur une idée très concrète : une image disque, c’est juste une suite d’octets. Donc au lieu de “monter” un filesystem avec le système, je lis et j’interprète ces octets moi-même.

Je commence par lire le BPB dans le tout premier secteur (512 octets). Dedans je récupère des informations qui sont indispensables pour tout le reste : la taille d’un secteur, le nombre de secteurs par cluster, le nombre de FAT, la taille d’une FAT, et le cluster racine. Ces valeurs me servent ensuite à calculer où commence la FAT dans l’image, et où commence la zone data.

À partir de là, quand je parle d’un cluster, je peux vraiment calculer son emplacement dans le buffer mémoire. Un cluster, c’est `bytes_per_sector * sectors_per_cluster`. Donc si je connais le “début de la zone data”, je peux faire le calcul et aller lire exactement au bon endroit.

Pour lister un répertoire, je fais la même logique que FAT32 : je lis un cluster, je le découpe en blocs de 32 octets, parce qu’une entrée de répertoire fait 32 octets. Ensuite je reconstruis le nom 8.3, je lis les attributs (fichier ou répertoire), je récupère le premier cluster, et la taille si c’est un fichier. Je m’arrête quand je tombe sur l’entrée `0x00`, parce que dans FAT32 ça signifie “fin du répertoire”.

Pour lire un fichier, je fais encore quelque chose de très “bas niveau”. Je pars du premier cluster du fichier, puis je suis la chaîne dans la FAT. Une entrée FAT32 fait 4 octets, donc je lis à l’offset `fat_start + cluster*4`. J’enchaîne les clusters jusqu’à une valeur de fin (EOC). Ensuite je recopie les bytes des clusters dans un `Vec<u8>`, et je m’arrête exactement à la taille indiquée par l’entrée de répertoire.

---

## Comment j’ai organisé le projet

J’ai séparé le projet en deux parties parce que je voulais garder une base propre.

La bibliothèque `fat32_parser` contient la logique FAT32. Elle ne fait aucune entrée/sortie, elle ne dépend pas de `std`, et elle travaille uniquement sur un buffer en mémoire (`&[u8]` pour la lecture et `&mut [u8]` pour l’écriture). Ce choix rend la logique facile à tester et facile à réutiliser.

Le binaire `fat32_cli` est volontairement minimal. Son rôle est juste de lire `disk.img` depuis le disque, d’appeler la bibliothèque, et d’afficher le résultat. Je l’ai ajouté parce que ça me permet de démontrer le projet sur une vraie image FAT32, pas seulement sur un test.

---

## Ce que j’ai ajouté en plus : écriture persistante

Au début je faisais surtout de la lecture, mais j’ai décidé d’aller plus loin et d’implémenter aussi une écriture simple et réelle.

Concrètement, j’ai une structure `Fat32Mut` qui travaille sur `&mut [u8]`. Avec ça, je peux créer un fichier (ou écraser un fichier existant) dans un répertoire déjà présent, et écrire son contenu directement dans l’image. Ensuite, la CLI réécrit `disk.img` sur le disque, donc la modification reste.

Je suis resté sur une écriture volontairement simple. Je supporte uniquement les noms 8.3, je ne crée pas encore de répertoires, et je n’implémente pas les timestamps. Par contre, ce que j’ai fait est “vrai” : j’alloue des clusters en scannant la FAT, je chaîne les clusters dans la FAT, j’écris les bytes dans la zone data, et je mets à jour l’entrée de répertoire. Et si j’écrase un fichier existant, je libère correctement l’ancienne chaîne de clusters.

C’est aussi pour ça que je n’ai pas ajouté `mkdir` dans la lib : créer un répertoire, c’est créer une entrée de répertoire + gérer les entrées `.` et `..` + potentiellement allouer un cluster pour le répertoire + gérer l’extension du répertoire si on manque de place. J’ai préféré sécuriser d’abord la partie “write file” correctement, parce que c’est déjà la partie la plus sensible.

---

## Les Fonctions importantes 

La première étape, c’est `parse_bpb`. C’est là que je lis les champs essentiels du BPB directement dans les bytes du secteur 0. Sans ça, je ne peux pas calculer où se trouve la FAT ni où se trouve la zone data.

Ensuite, `Fat32::new` et `Fat32Mut::new` construisent une vue cohérente du volume. Elles stockent les paramètres dont tout le reste a besoin, comme la taille d’un cluster et les offsets de base.

La logique la plus importante au quotidien, c’est la conversion cluster → offset. C’est ce qui me permet de lire un cluster avec `read_cluster`, donc de lire un répertoire ou le contenu d’un fichier.

Pour la navigation, `open_path` est centrale. Elle prend un chemin absolu comme `/DIR/NOTE.TXT` et avance segment par segment. À chaque segment, je liste le répertoire courant, je compare les noms, et je passe au cluster suivant. Je normalise en majuscules pour être cohérent avec le comportement FAT sur les noms courts.

Pour lister, `list_dir_cluster` lit la chaîne de clusters du répertoire via la FAT, puis parcourt les entrées 32 bytes par 32 bytes. C’est l’étape où je reconstruis les `DirEntry`.

Pour lire, `read_file` suit la chaîne de clusters d’un fichier et reconstruit le contenu jusqu’à la taille annoncée. Je mets aussi une limite maximale de clusters parcourus pour éviter une boucle infinie sur une image corrompue.

Et pour l’écriture, la fonction la plus importante est `write_file_by_path`. Elle vérifie le chemin, récupère le répertoire parent, cherche si le fichier existe, libère l’ancienne chaîne si besoin, alloue des clusters libres, écrit les bytes dans la data, puis met à jour (ou crée) l’entrée de répertoire. Ensuite, comme la CLI sauvegarde le buffer modifié dans `disk.img`, l’écriture est permanente.

---

## Tests

J’ai construit des tests obligatoires avec `cargo test`. Pour ça, j’ai une petite image FAT32 synthétique en mémoire. Elle contient un BPB minimal, une FAT cohérente, une racine avec `HELLO.TXT` et un dossier `DIR`, et quelques clusters libres.

L’intérêt de cette approche, c’est que je contrôle totalement l’image. Je peux valider la lecture, la résolution de chemin, les erreurs attendues, et aussi l’écriture. Par exemple, je teste qu’un fichier peut être créé, qu’un fichier peut être écrasé, et que l’ancienne chaîne de clusters est bien libérée dans la FAT.

---

## Créer une image FAT32 réelle pour tester

Je commence par créer un fichier image rempli de zéros avec `dd`.

```bash
dd if=/dev/zero of=disk.img bs=1M count=64
```

Ensuite j’installe l’outil de formatage et je formate l’image en FAT32.

```bash
sudo apt update
sudo apt install dosfstools
mkfs.vfat -F 32 disk.img
```

Je monte l’image en loop pour ajouter des fichiers.

```bash
sudo mkdir -p /mnt/fat32_test
sudo mount -o loop disk.img /mnt/fat32_test
```

Je crée du contenu de test.

```bash
echo "Hello FAT32" | sudo tee /mnt/fat32_test/HELLO.TXT > /dev/null
sudo mkdir -p /mnt/fat32_test/DIR
echo "Inside DIR" | sudo tee /mnt/fat32_test/DIR/NOTE.TXT > /dev/null
```

Je démonte l’image avant de la passer à la CLI.

```bash
sudo umount /mnt/fat32_test
```

---

## Lancer les tests

```bash
cargo test
```

Si je veux voir les sorties :

```bash
cargo test -- --nocapture
```

---

## Utiliser la CLI

Je compile en release :

```bash
cargo build --release
```

Je liste et je lis :

```bash
./target/release/fat32_cli --file disk.img --ls /
./target/release/fat32_cli --file disk.img --cat /HELLO.TXT
```

Je peux aussi écrire un fichier dans l’image (c’est persistant) :

```bash
echo "DATA" > local.txt
./target/release/fat32_cli --file disk.img --put /NEW.TXT local.txt
./target/release/fat32_cli --file disk.img --cat /NEW.TXT
```

Je peux enfin utiliser le mode shell pour naviguer comme dans un mini terminal :

```bash
./target/release/fat32_cli --file disk.img
```

---

## Bonus Miri

J’ai lancé Miri pour faire une vérification mémoire plus stricte. Pour ça, j’ai installé le nightly, ajouté Miri, et fait le setup.

```bash
rustup toolchain install nightly
rustup +nightly component add miri
rustup +nightly miri setup
cargo +nightly miri test
```
