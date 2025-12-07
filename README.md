# FAT32_parsing

réimplémentation **FAT32 en lecture seule** en Rust
L'objectif de ce projet est de parser un volume FAT32 à partir d'un dump brut (`disk.img`), de lister des fichiers, de lire leur contenu, et de naviguer dans l’arborescence avec une petite CLI (`ls`, `cd`, `cat`, `pwd`).
--- 

## Démarche

Avant de coder, je suis parti relire des ressources comme **phil-opp (Writing an OS in Rust)** pour me remettre dans le contexte `no_std`, voir comment séparer une bibliothèque bas niveau d’un binaire, et comment utiliser `alloc` proprement sans la lib standard.

En parallèle, je me suis replongé dans la structure de **FAT32** : les champs importants du BPB (taille des secteurs, clusters, secteurs réservés, taille d’une FAT, cluster racine), la manière dont la FAT enchaîne les clusters et le format des entrées de répertoire (32 octets, noms en 8.3).

À partir de là, j’ai choisi une architecture assez classique : une lib `no_std + alloc` qui connaît FAT32 et ne manipule que des slices en mémoire, et un binaire `std` très fin qui lit le fichier `disk.img`, instancie la lib et offre une petite CLI pour jouer avec le système de fichiers.

---

La bibliothèque s’appelle `fat32_parser`.  
Elle est en `no_std` (hors tests) et ne dépend que de `core` et `alloc`.  
Elle expose une structure `Fat32<'a>` qui prend un `&[u8]` représentant le volume complet, et des fonctions pour lister des répertoires et lire des fichiers.

 
Le binaire s’appelle `fat32_cli`.  
Il utilise `std` uniquement pour lire `disk.img`, gérer `stdin` / `stdout` et parser les arguments.  
Une fois le volume chargé en mémoire, il lance un petit shell avec les commandes : `ls`, `cd`, `cat`, `pwd`, `help`, `exit`.

---

## Fonctions principales

`Fat32::new(&[u8])`  
Construit la vue du système de fichiers à partir du buffer brut
Elle lit le BPB, vérifie les champs de base (taille des secteurs, FAT, cluster racine, etc.) et prépare tous les offsets dont la lib a besoin.

`Fat32::list_root()` et `Fat32::list_dir_path("/CHEMIN")`  
Permettent de lister un répertoire.  
Dans les deux cas, la lib suit la chaîne de clusters du répertoire, parcourt les entrées de 32 octets, filtre les entrées libres / supprimées, reconstruit les noms 8.3 et retourne une liste de `DirEntry`.

`Fat32::open_path("/CHEMIN")`  
Résout un chemin absolu en une entrée de répertoire.  
La fonction avance segment par segment dans l’arborescence à partir du cluster racine et renvoie l’entrée correspondante si elle existe.

`Fat32::read_file_by_path("/CHEMIN")` et `Fat32::read_file(&DirEntry)`  
Gèrent la lecture des fichiers.  
La première résout le chemin, vérifie que c’est bien un fichier et retourne le contenu en mémoire.  
La seconde part d’un `DirEntry` et suit la chaîne de clusters dans la FAT pour reconstruire les octets du fichier.

Côté CLI, une fonction `resolve_path(current_dir, chemin)` reconstruit un chemin absolu en tenant compte du répertoire courant, de `.` et `..`.  
La CLI ne parle à la lib qu’en chemins absolus, ce qui simplifie la logique dans `fat32_parser`.
## Tests et dump

Pour les tests unitaires (`cargo test`), j’utilise un petit volume FAT32 synthétique généré en mémoire dans un tableau `[u8; N]`.  
Il contient un BPB minimal, une FAT très simple, un répertoire racine et un fichier `HELLO.TXT`.  
Cela permet de vérifier : la lecture de l’en-tête, le listage de la racine, la lecture de `HELLO.TXT`, la gestion d’un chemin inexistant ou d’un `NotAFile`.

Pour les essais “réels”, j’utilise une image `disk.img` créée avec `mkfs.vfat`, montée sous Linux, dans laquelle je crée quelques fichiers et répertoires avant de la démonter.  
Le binaire `fat32_cli` lit ensuite ce fichier et permet de tester `ls`, `cd`, `cat` et `pwd` sur un vrai volume FAT32.

---


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
fat32:/> ls
fat32:/> cd DIR
fat32:/DIR> ls
fat32:/DIR> cat NOTE.TXT
fat32:/DIR> pwd
fat32:/DIR> exit

