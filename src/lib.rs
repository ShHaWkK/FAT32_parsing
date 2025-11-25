#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::{string::String, vec::Vec};

mod dir_entry;

pub use dir_entry::{Attributes, DirEntry};

/// Erreurs possibles pendant l'analyse ou la lecture d'un dump FAT32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatError {
    /// Le buffer fourni est trop petit pour contenir les métadonnées nécessaires.
    BufferTooSmall,
    /// Le dump ne ressemble pas à un volume FAT32 valide.
    NotFat32,
    /// Lecture en dehors des limites du buffer.
    OutOfBounds,
    /// Numéro de cluster invalide (ex. < 2).
    InvalidCluster,
    /// L'entrée visée n'est pas un fichier alors qu'on essaie de la lire.
    NotAFile,
    /// L'entrée visée n'est pas un répertoire alors qu'on essaie de la lister.
    NotADirectory,
    /// Le chemin fourni ne correspond à aucune entrée.
    PathNotFound,
    /// Erreur générique pour les cas non couverts.
    Other,
}

/// Vue en lecture seule d'un volume FAT32 contenu dans un dump mémoire.
///
/// Le principe d'utilisation est simple :
///
///  Le binaire lit un fichier image FAT32 (`disk.img`) dans un `Vec<u8>`.
/// On construit un [`Fat32`] avec un `&[u8]` vers ce buffer.
///  On utilise les méthodes de haut niveau 
/// pour interagir avec le système de fichiers.
pub struct Fat32<'a> {
    disk: &'a [u8],

    // Informations extraites du premier secteur (BPB simplifié).
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    sectors_per_fat: u32,
    root_cluster: u32,
}

impl<'a> Fat32<'a> {
    /// Construit une instance à partir d'un dump complet de volume FAT32 en mémoire.
    /// Le buffer `disk` doit contenir l'intégralité du volume (comme un fichier .img).
    pub fn new(disk: &'a [u8]) -> Result<Self, FatError> {
        if disk.len() < 512 {
            return Err(FatError::BufferTooSmall);
        }

        let b = &disk[0..512];

        let bytes_per_sector = u16::from_le_bytes([b[11], b[12]]);
        let sectors_per_cluster = b[13];
        let reserved_sectors = u16::from_le_bytes([b[14], b[15]]);
        let num_fats = b[16];

        let total_sectors_16 = u16::from_le_bytes([b[19], b[20]]);
        let total_sectors_32 =
            u32::from_le_bytes([b[32], b[33], b[34], b[35]]);
        let _total_sectors = if total_sectors_16 != 0 {
            total_sectors_16 as u32
        } else {
            total_sectors_32
        };

        let sectors_per_fat =
            u32::from_le_bytes([b[36], b[37], b[38], b[39]]);
        let root_cluster =
            u32::from_le_bytes([b[44], b[45], b[46], b[47]]);

        // En FAT32, `sectors_per_fat` doit être non nul.
        if sectors_per_fat == 0 {
            return Err(FatError::NotFat32);
        }

        Ok(Self {
            disk,
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            sectors_per_fat,
            root_cluster,
        })
    }

    /// Liste le contenu du répertoire racine du volume.
    pub fn list_root(&self) -> Result<Vec<DirEntry>, FatError> {
        self.list_dir_cluster(self.root_cluster)
    }

    /// Liste le contenu d'un répertoire à partir de son chemin absolu.
    pub fn list_dir_path(&self, path: &str) -> Result<Vec<DirEntry>, FatError> {
        if path == "/" {
            return self.list_root();
        }

        let entry = self
            .open_path(path)?
            .ok_or(FatError::PathNotFound)?;

        if !entry.is_dir() {
            return Err(FatError::NotADirectory);
        }

        self.list_dir_cluster(entry.first_cluster)
    }

    /// Lit le contenu d'un fichier à partir de son chemin absolu.
    ///
    /// Retourne :
    /// - `Ok(Some(Vec<u8>))` si le fichier existe,
    /// - `Ok(None)` si le chemin ne correspond à rien,
    /// - `Err(_)` si le chemin existe mais ne désigne pas un fichier.
    pub fn read_file_by_path(
        &self,
        path: &str,
    ) -> Result<Option<Vec<u8>>, FatError> {
        let entry = match self.open_path(path)? {
            Some(e) => e,
            None => return Ok(None),
        };

        if !entry.is_file() {
            return Err(FatError::NotAFile);
        }

        let content = self.read_file(&entry)?;
        Ok(Some(content))
    }

    /// Résout un chemin absolu (ex: `/HELLO.TXT` ou `/DIR/FILE.TXT`) en entrée de répertoire.
    pub fn open_path(&self, path: &str) -> Result<Option<DirEntry>, FatError> {
        if !path.starts_with('/') {
            return Err(FatError::Other);
        }

        let mut current_cluster = self.root_cluster;
        let mut last_entry: Option<DirEntry> = None;

        // Découpe du chemin en segments non vides.
        let parts = path.split('/').filter(|s| !s.is_empty());

        for part in parts {
            let target_name = Self::normalize_name(part);
            let entries = self.list_dir_cluster(current_cluster)?;
            let mut found = None;

            for e in entries {
                if Self::normalize_name(&e.name) == target_name {
                    current_cluster = e.first_cluster;
                    found = Some(e);
                    break;
                }
            }

            match found {
                Some(e) => last_entry = Some(e),
                None => return Ok(None),
            }
        }

        Ok(last_entry)
    }

    /// Lit le contenu d'un fichier à partir de l'entrée de répertoire correspondante.
    pub fn read_file(&self, entry: &DirEntry) -> Result<Vec<u8>, FatError> {
        if !entry.is_file() {
            return Err(FatError::NotAFile);
        }

        let cluster_size = self.cluster_size();
        let mut data = Vec::new();
        let mut remaining = entry.size as usize;

        let chain = self.follow_chain(entry.first_cluster, 4096)?;

        for cl in chain {
            let cluster = self.read_cluster(cl)?;
            let to_take = core::cmp::min(remaining, cluster_size);
            data.extend_from_slice(&cluster[..to_take]);
            remaining -= to_take;
            if remaining == 0 {
                break;
            }
        }

        Ok(data)
    }

    // ---------- Méthodes internes : calculs d'offsets, FAT, etc. ----------

    fn bytes_per_sector(&self) -> usize {
        self.bytes_per_sector as usize
    }

    fn cluster_size(&self) -> usize {
        self.bytes_per_sector() * self.sectors_per_cluster as usize
    }

    fn fat_start_byte(&self) -> usize {
        self.reserved_sectors as usize * self.bytes_per_sector()
    }

    fn data_start_byte(&self) -> usize {
        self.fat_start_byte()
            + (self.num_fats as usize * self.sectors_per_fat as usize)
                * self.bytes_per_sector()
    }

    fn cluster_to_offset(&self, cluster: u32) -> Result<usize, FatError> {
        if cluster < 2 {
            return Err(FatError::InvalidCluster);
        }

        let index = (cluster - 2) as usize;
        let offset = self.data_start_byte() + index * self.cluster_size();

        if offset >= self.disk.len() {
            return Err(FatError::OutOfBounds);
        }

        Ok(offset)
    }

    fn read_cluster(&self, cluster: u32) -> Result<&[u8], FatError> {
        let offset = self.cluster_to_offset(cluster)?;
        let size = self.cluster_size();

        if offset + size > self.disk.len() {
            return Err(FatError::OutOfBounds);
        }

        Ok(&self.disk[offset..offset + size])
    }

    fn read_fat_entry(&self, cluster: u32) -> Result<u32, FatError> {
        let fat_start = self.fat_start_byte();
        let entry_offset = fat_start + cluster as usize * 4;

        if entry_offset + 4 > self.disk.len() {
            return Err(FatError::OutOfBounds);
        }

        let bytes = &self.disk[entry_offset..entry_offset + 4];
        let val = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        Ok(val & 0x0FFF_FFFF)
    }

    fn follow_chain(
        &self,
        start_cluster: u32,
        max_clusters: usize,
    ) -> Result<Vec<u32>, FatError> {
        let mut result = Vec::new();
        let mut current = start_cluster;

        for _ in 0..max_clusters {
            if current < 2 {
                return Err(FatError::InvalidCluster);
            }

            result.push(current);

            let next = self.read_fat_entry(current)?;
            if next >= 0x0FFF_FFF8 {
                // Fin de chaîne.
                break;
            }

            current = next;
        }

        Ok(result)
    }

    fn list_dir_cluster(
        &self,
        start_cluster: u32,
    ) -> Result<Vec<DirEntry>, FatError> {
        let cluster_size = self.cluster_size();
        let mut entries = Vec::new();

        let chain = self.follow_chain(start_cluster, 4096)?;

        for cl in chain {
            let data = self.read_cluster(cl)?;

            for chunk in data[..cluster_size].chunks(32) {
                if let Some(entry) = DirEntry::parse(chunk) {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }

    /// Normalise un nom pour la comparaison (ASCII upper-case).
    fn normalize_name(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for ch in s.chars() {
            out.push(ch.to_ascii_uppercase());
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Construit un petit dump FAT32 en mémoire pour les tests.
    ///
    /// Layout :
    /// - secteur 0 : entête avec les champs FAT32 nécessaires,
    /// - secteur 1 : FAT,
    /// - secteur 2 : répertoire racine avec une entrée HELLO.TXT,
    /// - secteur 3 : données du fichier ("HELLO").
    fn build_test_image() -> [u8; 2048] {
        const SECTOR_SIZE: usize = 512;
        const NUM_SECTORS: usize = 4;
        let mut disk = [0u8; SECTOR_SIZE * NUM_SECTORS];

        {
            let b = &mut disk[0..SECTOR_SIZE];

            // bytes_per_sector = 512 (0x0200 LE)
            b[11] = 0x00;
            b[12] = 0x02;

            // sectors_per_cluster = 1
            b[13] = 0x01;

            // reserved_sectors = 1
            b[14] = 0x01;
            b[15] = 0x00;

            // num_fats = 1
            b[16] = 0x01;

            // sectors_per_fat = 1 (u32 LE)
            b[36] = 0x01;
            b[37] = 0x00;
            b[38] = 0x00;
            b[39] = 0x00;

            // root_cluster = 2
            b[44] = 0x02;
            b[45] = 0x00;
            b[46] = 0x00;
            b[47] = 0x00;
        }

        // --- secteur 1 : FAT ---
        {
            let fat_start = SECTOR_SIZE; // reserved_sectors(1) * 512
            let fat = &mut disk[fat_start..fat_start + SECTOR_SIZE];

            let eoc: u32 = 0x0FFF_FFFF;
            let eoc_bytes = eoc.to_le_bytes();

            // cluster 2 (racine) → EOC
            let c2_offset = 2 * 4;
            fat[c2_offset..c2_offset + 4].copy_from_slice(&eoc_bytes);

            // cluster 3 (fichier HELLO.TXT) → EOC
            let c3_offset = 3 * 4;
            fat[c3_offset..c3_offset + 4].copy_from_slice(&eoc_bytes);
        }

        // --- secteur 2 : cluster racine (cluster 2) ---
        {
            // data_start = reserved(1) + num_fats(1) * spf(1) = secteur 2
            let root_cluster_sector = 2;
            let root_off = root_cluster_sector * SECTOR_SIZE;
            let dir = &mut disk[root_off..root_off + SECTOR_SIZE];

            // Entrée 0 : fichier HELLO.TXT
            let mut entry = [0u8; 32];

            // "HELLO   " + "TXT" en 8.3
            entry[0..8].copy_from_slice(b"HELLO   ");
            entry[8..11].copy_from_slice(b"TXT");

            // Attributs : archive (0x20)
            entry[11] = 0x20;

            // first_cluster_high (20..22) = 0
            entry[20] = 0x00;
            entry[21] = 0x00;

            // first_cluster_low (26..28) = 3 (cluster 3)
            entry[26] = 0x03;
            entry[27] = 0x00;

            // taille = 5 octets ("HELLO")
            entry[28] = 5;
            entry[29] = 0;
            entry[30] = 0;
            entry[31] = 0;

            dir[0..32].copy_from_slice(&entry);

            // Entrée suivante : 0x00 → fin de répertoire
            dir[32] = 0x00;
        }

        // --- secteur 3 : données du fichier (cluster 3) ---
        {
            let file_cluster_sector = 3; // data_start(2) + (3-2)
            let file_off = file_cluster_sector * SECTOR_SIZE;
            let data = &mut disk[file_off..file_off + SECTOR_SIZE];

            data[0..5].copy_from_slice(b"HELLO");
        }

        disk
    }

    /// Vérifie que `Fat32::new` récupère bien les champs critiques.
    #[test]
    fn parse_header_ok() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        assert_eq!(fs.bytes_per_sector, 512);
        assert_eq!(fs.sectors_per_cluster, 1);
        assert_eq!(fs.reserved_sectors, 1);
        assert_eq!(fs.num_fats, 1);
        assert_eq!(fs.sectors_per_fat, 1);
        assert_eq!(fs.root_cluster, 2);
    }

    /// Vérifie que la racine contient HELLO.TXT et que son contenu est correct.
    #[test]
    fn list_root_and_read_file() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        let root = fs.list_root().expect("list_root failed");
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].name, "HELLO.TXT");
        assert_eq!(root[0].size, 5);

        let content = fs
            .read_file_by_path("/HELLO.TXT")
            .expect("read_file_by_path failed")
            .expect("file not found");

        assert_eq!(content, b"HELLO");
    }

    /// Un répertoire inexistant doit renvoyer une erreur PathNotFound.
    #[test]
    fn list_dir_unknown_path_returns_path_not_found() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        let err = fs.list_dir_path("/DOES_NOT_EXIST").unwrap_err();
        assert_eq!(err, FatError::PathNotFound);
    }

    /// Un fichier inexistant renvoie Ok(None) via read_file_by_path.
    #[test]
    fn read_file_by_path_unknown_returns_none() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        let res = fs
            .read_file_by_path("/MISSING.TXT")
            .expect("read_file_by_path failed");

        assert!(res.is_none());
    }

    /// Un chemin relatif doit être refusé (on impose les chemins absolus).
    #[test]
    fn open_path_relative_returns_error() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        let err = fs.open_path("HELLO.TXT").unwrap_err();
        assert_eq!(err, FatError::Other);
    }

    /// Appeler read_file sur une entrée marquée comme répertoire doit renvoyer NotAFile.
    #[test]
    fn read_file_on_directory_returns_not_a_file() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        // DirEntry artificiel marquant un répertoire.
        let attrs = Attributes {
            read_only: false,
            hidden: false,
            system: false,
            volume_id: false,
            directory: true,
            archive: false,
        };

        let dir_entry = DirEntry {
            name: "ROOT".into(),
            attrs,
            first_cluster: fs.root_cluster,
            size: 0,
        };

        let res = fs.read_file(&dir_entry);
        assert!(matches!(res, Err(FatError::NotAFile)));
    }
}
