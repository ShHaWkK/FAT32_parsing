//! Entrées de répertoire FAT32 

extern crate alloc;

use alloc::string::String;

/// Attributs FAT d'une entrée de répertoire.
#[derive(Debug, Clone, Copy)]
pub struct Attributes {
    pub read_only: bool,
    pub hidden: bool,
    pub system: bool,
    pub volume_id: bool,
    pub directory: bool,
    pub archive: bool,
}

impl Attributes {
    /// Construit les attributs à partir de l'octet brut.
    pub fn from_byte(b: u8) -> Self {
        Self {
            read_only: b & 0x01 != 0,
            hidden: b & 0x02 != 0,
            system: b & 0x04 != 0,
            volume_id: b & 0x08 != 0,
            directory: b & 0x10 != 0,
            archive: b & 0x20 != 0,
        }
    }
}

/// Entrée de répertoire FAT32 avec nom court
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub attrs: Attributes,
    pub first_cluster: u32,
    pub size: u32,
}

impl DirEntry {
    /// Parse une entrée de 32 octets.
    pub fn parse(entry: &[u8]) -> Option<Self> {
        if entry.len() < 32 {
            return None;
        }

        if entry[0] == 0x00 || entry[0] == 0xE5 {
            return None;
        }

        let attrs = Attributes::from_byte(entry[11]);
        if attrs.volume_id {
            return None;
        }

        let name_raw = &entry[0..8];
        let ext_raw = &entry[8..11];

        let name = decode_ascii_trim(name_raw);
        let ext = decode_ascii_trim(ext_raw);

        let full_name = if !ext.is_empty() {
            let mut s =
                String::with_capacity(name.len() + 1 + ext.len());
            s.push_str(&name);
            s.push('.');
            s.push_str(&ext);
            s
        } else {
            name
        };

        let first_cluster_high =
            u16::from_le_bytes([entry[20], entry[21]]) as u32;
        let first_cluster_low =
            u16::from_le_bytes([entry[26], entry[27]]) as u32;
        let first_cluster = (first_cluster_high << 16) | first_cluster_low;

        let size = u32::from_le_bytes([
            entry[28],
            entry[29],
            entry[30],
            entry[31],
        ]);

        Some(Self {
            name: full_name,
            attrs,
            first_cluster,
            size,
        })
    }

    /// Indique si l'entrée est un répertoire.
    pub fn is_dir(&self) -> bool {
        self.attrs.directory
    }

    /// Indique si l'entrée est un fichier.
    pub fn is_file(&self) -> bool {
        !self.attrs.directory
    }
}

/// Décodage ASCII simple avec suppression des espaces de fin.
fn decode_ascii_trim(bytes: &[u8]) -> String {
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b' ' {
        end -= 1;
    }

    let mut s = String::with_capacity(end);
    for &b in &bytes[..end] {
        s.push(b as char);
    }
    s
}
