# FAT32_parsing

réimplémentation **FAT32 en lecture seule** en Rust
L'objectif de ce projet est de parser un volume FAT32 à partir d'un dump brut (`disk.img`), de lister des fichiers, de lire leur contenu, et de naviguer dans l’arborescence avec une petite CLI (`ls`, `cd`, `cat`, `pwd`).
--- 

## Ma démarche 

Avant d'écrire la moindre ligne de code, j'ai d'abord pris le temps de me documenter. 

**phil-opp (Writing an OS in Rust)**  
   J’ai relu les parties qui parlent de :
   - environnement `no_std`,
   - séparation entre une bibliothèque “bas niveau” et un binaire,
   - utilisation de `alloc` sans la lib standard,
   - comment structurer un projet pour qu’il soit réutilisable dans un contexte type OS.

**Documentation FAT32 / BPB / FAT**  
   Ensuite je me suis concentré sur FAT32 lui-même :
   - champs importants du BPB (bytes per sector, sectors per cluster, reserved sectors, FAT size, root cluster…),
   - organisation de la FAT,
   - comment calculer l’offset d’un cluster dans la zone de données,
   - comment lire les entrées de répertoire (32 octets, format 8.3).

 À partir de là, j’ai décidé de faire une **lib `fat32_parser` en `no_std + alloc`**, qui ne fait que du FAT32 en mémoire, faire un **binaire `fat32_cli`** qui lit un fichier `disk.img`, instancie la lib et propose une petite CLI pour la correction (`ls`, `cd`, `cat`, `pwd`) et pour avoir aussi avoir un visuel. 
 
--- 


 
