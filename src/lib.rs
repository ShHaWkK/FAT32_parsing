//! Parseur FAT32 
//!
//! Cette bibliothèque est "no_std" (hors tests) et ne repose que sur "core" et "alloc"


#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::{string::String, vec::Vec};

mod dir_entry;

pub use dir_entry::{Attributes, DirEntry};

/// Erreurs possibles lors de la lecture du système de fichiers FAT32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatError {
    BufferTooSmall,
    NotFat32,
    OutOfBounds,
    InvalidCluster,
    NotAFile,
    NotADirectory,
    PathNotFound,
    Other,
}

/// Vue en lecture seule d'un volume FAT32 stocké dans un buffer mémoire.
pub struct Fat32<'a> {
    disk: &'a [u8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    sectors_per_fat: u32,
    root_cluster: u32,
}

impl<'a> Fat32<'a> {
    /// Construit une vue FAT32 depuis un dump en mémoire.
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

    /// Liste le contenu du répertoire racine.
    pub fn list_root(&self) -> Result<Vec<DirEntry>, FatError> {
        self.list_dir_cluster(self.root_cluster)
    }

    /// Liste un répertoire à partir d'un chemin absolu (ex: `/DIR`).
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

    /// Lit un fichier à partir de son chemin absolu.
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

    /// Résout un chemin absolu en entrée de répertoire (sans lire son contenu).
    pub fn open_path(&self, path: &str) -> Result<Option<DirEntry>, FatError> {
        if !path.starts_with('/') {
            return Err(FatError::Other);
        }

        let mut current_cluster = self.root_cluster;
        let mut last_entry: Option<DirEntry> = None;

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

    /// Lit un fichier à partir de l'entrée de répertoire associée.
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

    // ---------- Méthodes internes ----------

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

    /// Mini volume FAT32 en mémoire :
    /// - secteur 0 : BPB simplifié,
    /// - secteur 1 : FAT,
    /// - secteur 2 : racine avec HELLO.TXT,
    /// - secteur 3 : contenu "HELLO".
    fn build_test_image() -> [u8; 2048] {
        const SECTOR_SIZE: usize = 512;
        const NUM_SECTORS: usize = 4;
        let mut disk = [0u8; SECTOR_SIZE * NUM_SECTORS];

        // secteur 0 : BPB
        {
            let b = &mut disk[0..SECTOR_SIZE];

            b[11] = 0x00; // bytes_per_sector = 512 (0x0200 LE)
            b[12] = 0x02;

            b[13] = 0x01; // sectors_per_cluster = 1

            b[14] = 0x01; // reserved_sectors = 1
            b[15] = 0x00;

            b[16] = 0x01; // num_fats = 1

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

        // secteur 1 : FAT
        {
            let fat_start = SECTOR_SIZE;
            let fat = &mut disk[fat_start..fat_start + SECTOR_SIZE];

            let eoc: u32 = 0x0FFF_FFFF;
            let eoc_bytes = eoc.to_le_bytes();

            // cluster 2 (racine) -> EOC
            let c2_offset = 2 * 4;
            fat[c2_offset..c2_offset + 4].copy_from_slice(&eoc_bytes);

            // cluster 3 (fichier HELLO.TXT) -> EOC
            let c3_offset = 3 * 4;
            fat[c3_offset..c3_offset + 4].copy_from_slice(&eoc_bytes);
        }

        // secteur 2 : racine (cluster 2)
        {
            let root_cluster_sector = 2;
            let root_off = root_cluster_sector * SECTOR_SIZE;
            let dir = &mut disk[root_off..root_off + SECTOR_SIZE];

            let mut entry = [0u8; 32];

            entry[0..8].copy_from_slice(b"HELLO   ");
            entry[8..11].copy_from_slice(b"TXT");

            entry[11] = 0x20; // archive

            entry[20] = 0x00; // high
            entry[21] = 0x00;

            entry[26] = 0x03; // low = 3
            entry[27] = 0x00;

            entry[28] = 5; // size = 5 ("HELLO")
            entry[29] = 0;
            entry[30] = 0;
            entry[31] = 0;

            dir[0..32].copy_from_slice(&entry);
            dir[32] = 0x00; // fin de répertoire
        }

        // secteur 3 : contenu du fichier (cluster 3)
        {
            let file_cluster_sector = 3;
            let file_off = file_cluster_sector * SECTOR_SIZE;
            let data = &mut disk[file_off..file_off + SECTOR_SIZE];

            data[0..5].copy_from_slice(b"HELLO");
        }

        disk
    }

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

    #[test]
    fn list_dir_unknown_path_returns_path_not_found() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        let err = fs.list_dir_path("/DOES_NOT_EXIST").unwrap_err();
        assert_eq!(err, FatError::PathNotFound);
    }

    #[test]
    fn read_file_by_path_unknown_returns_none() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        let res = fs
            .read_file_by_path("/MISSING.TXT")
            .expect("read_file_by_path failed");

        assert!(res.is_none());
    }

    #[test]
    fn open_path_relative_returns_error() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

        let err = fs.open_path("HELLO.TXT").unwrap_err();
        assert_eq!(err, FatError::Other);
    }

    #[test]
    fn read_file_on_directory_returns_not_a_file() {
        let disk = build_test_image();
        let fs = Fat32::new(&disk).expect("fat32 new failed");

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
