# FAT32_parsing

Ce projet est une réimplémentation FAT32 en lecture seule en Rust. L’objectif est de parser un volume FAT32 à partir d’un dump brut (par exemple disk.img), de lister des fichiers depuis un chemin donné, de lire le contenu d’un fichier depuis un chemin donné, et de proposer une manière simple de représenter où l’on se situe dans l’arborescence, à la manière d’un cd et d’un pwd.

--- 

## Démarche

Avant de coder, je suis parti relire des ressources comme **phil-opp (Writing an OS in Rust)** pour me remettre dans le contexte `no_std`, voir comment séparer une bibliothèque bas niveau d’un binaire, et comment utiliser `alloc` proprement sans la lib standard.

Ensuite, je me suis documenté sur FAT32 lui-même, en me concentrant surtout sur la partie utile au sujet. J’ai relu la structure du BPB (notamment la taille des secteurs, la taille des clusters, le nombre de FAT, la taille d’une FAT et le cluster racine), le principe d’enchaînement des clusters via la FAT, et le format des entrées de répertoire en 32 octets avec noms courts en 8.3. L’idée était de viser une implémentation lisible et robuste pour la lecture, plutôt que de vouloir couvrir tous les cas avancés du standard.

À partir de là, j’ai choisi une architecture assez classique : une lib `no_std + alloc` qui connaît FAT32 et ne manipule que des slices en mémoire, et un binaire `std` très fin qui lit le fichier `disk.img`, instancie la lib et offre une petite CLI pour jouer avec le système de fichiers.

---

## Architecture et choix techniques

J’ai séparé le projet en deux parties complémentaires



La bibliothèque `fat32_parser` contient toute la logique FAT32.
Elle est en `no_std` (hors tests) et utilise uniquement core et alloc. Elle travaille sur un &[u8] représentant le volume complet en mémoire. Ce choix permet de tester facilement la logique avec une image synthétique en RAM et rend la lib réutilisable dans un contexte proche OS ou embarqué.


Grossièrement elle expose une structure `Fat32<'a>` qui prend un `&[u8]` représentant le volume complet, et des fonctions pour lister des répertoires et lire des fichiers.

 
Le binaire fat32_cli est volontairement fin. Il utilise std seulement pour lire `disk.img`, gérer l’entrée/sortie et offrir une petite interface interactive. 

Cette CLI n’est pas obligatoire dans la consigne, mais je l’ai ajoutée pour une raison simple. Elle me permet de prouver visuellement que la bibliothèque fonctionne aussi sur une image FAT32 réelle et pas seulement sur un mini volume de test. Cela reste une démonstration optionnelle, sans jamais polluer le cœur `no_std`.
---

## Fonctions principales

La bibliothèque permet de lister un répertoire et de lire un fichier à partir d’un chemin absolu. La résolution de chemin repose sur une lecture des répertoires à partir du cluster racine, en comparant les noms qui sont normalisées en majuscules pour coller au comportement classique des noms courts FAT.


Dans la CLI, j’ai ajouté la gestion des chemins relatifs, `.` et `..`, afin de rendre les commandes proches d’un usage réel. La fonction resolve_path recompose un chemin absolu à partir du répertoire courant. Cela évite d’augmenter inutilement la complexité côté bibliothèque et maintient un découplage propre entre logique FAT32 et ergonomie utilisateur.

Explication des fonctions principales

`Fat32::new` construit une vue cohérente du volume à partir du buffer brut. Elle lit un BPB minimal et récupère les paramètres nécessaires au calcul des offsets. J’ai ajouté des vérifications simples pour éviter qu’un volume incohérent ou incomplet soit traité comme un FAT32 valide.

`Fat32::list_root` et `Fat32::list_dir_path` gèrent le listage des entrées d’un répertoire. La logique suit la chaîne de clusters du répertoire au travers de la FAT, puis parcourt les structures de 32 octets. Les entrées libres, supprimées ou de type volume label sont ignorées. Les noms sont reconstruits au format 8.3.

`Fat32::open_path` résout un chemin absolu sans lire le contenu d’un fichier. La fonction avance segment par segment dans l’arborescence, en listant le répertoire courant et en cherchant l’entrée correspondant au segment suivant. Ce découpage m’a permis d’avoir une seule fonction de résolution de chemin réutilisée par le listage et la lecture de fichier.

`Fat32::read_file_by_path` est un point d’entrée pratique. Elle résout le chemin puis vérifie qu’on pointe bien vers un fichier, avant d’appeler la logique de lecture.

`Fat32::read_file` reconstruit le contenu d’un fichier en suivant la chaîne de clusters. J’ai fixé une limite de parcours pour éviter une boucle infinie en cas de FAT corrompue. Le nombre d’octets réellement copiés est borné par la taille annoncée dans l’entrée de répertoire.


## Tests et dump

La consigne demande des tests obligatoires via `cargo test`. 

J’ai donc construit une suite d’unit tests basée sur un volume FAT32 synthétique en mémoire. 

Cette image contient un BPB minime, une FAT simple, un répertoire racine et un fichier HELLO.TXT. 

Cela me permet de valider la lecture de l’en-tête, le listage de la racine, la lecture d’un fichier, et plusieurs cas d’erreurs utiles.

J’ai aussi ajouté un test symétrique propre pour vérifier un comportement typique attendu par un utilisateur. Lister un chemin correspondant à un fichier doit déclencher `NotADirectory`, ce qui prouve que la lib ne mélange pas les types d’entrées.

Pour aller un cran plus loin, j’ai ajouté un test d’intégration optionnel dans `tests/disk_img.rs`. Son but est de vérifier que la bibliothèque fonctionne sur une vraie image FAT32 générée par un outil standard. Le test est volontairement tolérant. S’il ne trouve pas tests/disk.img, il n’échoue pas. Cela évite de forcer un fichier binaire dans le dépôt tout en laissant la possibilité d’une validation plus réaliste.

---

Commandes de test

Les tests attendus par la consigne se lancent simplement avec :

```
cargo test

```

Si je veux voir les messages éventuels imprimés par les tests d’intégration :

```
cargo test -- --nocapture
```

Commmandes : 

### Créer un fichier 

fallocate -l 64M disk.img
ou 
dd if=/dev/zero of=disk.img bs=1M count=64

### Formater l'image en FAT32 :

```
sudo apt update
sudo apt install dosfstools
```

Puis : 

```
mkfs.vfat -F 32 disk.img
```

### Monter l'image comme un vrai disque : 

```
sudo mkdir -p /mnt/fat32_test
sudo mount -o loop disk.img /mnt/fat32_test
```

### Créer quelques fichiers / répertoires pour tester


```
# fichier à la racine
echo "Hello FAT32" | sudo tee /mnt/fat32_test/HELLO.TXT

# un dossier
sudo mkdir -p /mnt/fat32_test/DIR

# un fichier dans DIR
echo "Inside DIR" | sudo tee /mnt/fat32_test/DIR/NOTE.TXT
```

On peux vérifier : 

```
ls -l /mnt/fat32_test
ls -l /mnt/fat32_test/DIR
```

!!!! Très important avant d'utiliser le bianire : 

```
sudo umount /mnt/fat32_test
```

À partir de là, le fichier disk.img dans le projet contient un vrai volume FAT32 valide.



Tester le binaire : 

```
cargo build --release
```

# mode "one shot"
./target/release/fat32_cli --file disk.img --ls /
./target/release/fat32_cli --file disk.img --ls DIR
./target/release/fat32_cli --file disk.img --cat /HELLO.TXT
./target/release/fat32_cli --file disk.img --cat DIR/NOTE.TXT

# mode shell 
./target/release/fat32_cli --file disk.img

puis : 
```
fat32:/> ls
fat32:/> cd DIR
fat32:/DIR> ls
fat32:/DIR> cat NOTE.TXT
fat32:/DIR> pwd
fat32:/DIR> exit
```


## Bonus Miri

Le sujet mentionne que Miri est un bonus. J’ai choisi de l’essayer pour montrer une démarche qualité supplémentaire. Cela ne remplace pas les tests classiques, mais apporte une vérification mémoire plus stricte.


Installation de Miri :

```
rustup +nightly component add miri
```

Puis lancement :

```
cargo +nightly miri test
```
