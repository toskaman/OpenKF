use crate::error::PackageError;
use bitflags::bitflags;

/// Standard Unreal Engine 2.5 package signature (Little-Endian: C2 83 2A 9E).
pub const PACKAGE_TAG: u32 = 0x9E2A83C2;

/// Canonical Unreal package signature (C1 83 2A 9E). Some KF packages
/// (e.g. KFGui.utx, CellExample.utx) are saved with it; layout is identical.
pub const PACKAGE_TAG_STANDARD: u32 = 0x9E2A83C1;

/// Standard Killing Floor retail engine version (128).
pub const ENGINE_VERSION_KF: u16 = 128;

/// Standard Killing Floor retail licensee version (29).
pub const LICENSEE_VERSION_KF: u16 = 29;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct PackageFlags: u32 {
        const ALLOW_DOWNLOAD  = 0x00000001;
        const CLIENT_OPTIONAL = 0x00000002;
        const SERVER_SIDE_ONLY= 0x00000004;
        const BROKEN_LINKS    = 0x00000008;
        const UNSECURE        = 0x00000010;
        const NEED            = 0x00008000;
    }
}

/// Package generation record tracking export and name count historical compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FGenerationInfo {
    pub export_count: i32,
    pub name_count: i32,
}

/// Unreal Engine 2.5 Package File Header (`FPackageFileSummary`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FPackageFileSummary {
    pub tag: u32,
    pub file_version: i32,
    pub package_flags: u32,
    pub name_count: i32,
    pub name_offset: i32,
    pub export_count: i32,
    pub export_offset: i32,
    pub import_count: i32,
    pub import_offset: i32,
    pub guid: [u8; 16],
    pub generation_count: i32,
    pub generations: Vec<FGenerationInfo>,
}

impl FPackageFileSummary {
    /// Returns the engine version (lower 16 bits of `file_version`).
    #[inline]
    pub fn engine_version(&self) -> u16 {
        (self.file_version & 0xFFFF) as u16
    }

    /// Returns the licensee version (upper 16 bits of `file_version`).
    #[inline]
    pub fn licensee_version(&self) -> u16 {
        ((self.file_version >> 16) & 0xFFFF) as u16
    }

    /// Deserializes `FPackageFileSummary` from raw byte data.
    pub fn read(data: &[u8], offset: &mut usize) -> Result<Self, PackageError> {
        let min_header_size = 0x38;
        if data.len() < min_header_size {
            return Err(PackageError::UnexpectedEof {
                offset: *offset,
                requested: min_header_size,
                total: data.len(),
            });
        }

        let tag = u32::from_le_bytes(data[0..4].try_into().unwrap());
        if tag != PACKAGE_TAG && tag != PACKAGE_TAG_STANDARD {
            return Err(PackageError::InvalidTag(tag));
        }

        let file_version = i32::from_le_bytes(data[4..8].try_into().unwrap());
        let eng_ver = (file_version & 0xFFFF) as u16;
        let lic_ver = ((file_version >> 16) & 0xFFFF) as u16;

        if eng_ver != ENGINE_VERSION_KF || lic_ver != LICENSEE_VERSION_KF {
            return Err(PackageError::UnsupportedVersion {
                engine: eng_ver,
                licensee: lic_ver,
            });
        }

        let package_flags = u32::from_le_bytes(data[8..12].try_into().unwrap());
        let name_count = i32::from_le_bytes(data[12..16].try_into().unwrap());
        let name_offset = i32::from_le_bytes(data[16..20].try_into().unwrap());
        let export_count = i32::from_le_bytes(data[20..24].try_into().unwrap());
        let export_offset = i32::from_le_bytes(data[24..28].try_into().unwrap());
        let import_count = i32::from_le_bytes(data[28..32].try_into().unwrap());
        let import_offset = i32::from_le_bytes(data[32..36].try_into().unwrap());

        let mut guid = [0u8; 16];
        guid.copy_from_slice(&data[36..52]);

        let generation_count = i32::from_le_bytes(data[52..56].try_into().unwrap());
        *offset = 56;

        let mut generations = Vec::with_capacity(generation_count.max(0) as usize);
        for _ in 0..generation_count {
            if *offset + 8 > data.len() {
                return Err(PackageError::UnexpectedEof {
                    offset: *offset,
                    requested: 8,
                    total: data.len(),
                });
            }
            let exp_c = i32::from_le_bytes(data[*offset..*offset + 4].try_into().unwrap());
            let name_c = i32::from_le_bytes(data[*offset + 4..*offset + 8].try_into().unwrap());
            *offset += 8;
            generations.push(FGenerationInfo {
                export_count: exp_c,
                name_count: name_c,
            });
        }

        Ok(Self {
            tag,
            file_version,
            package_flags,
            name_count,
            name_offset,
            export_count,
            export_offset,
            import_count,
            import_offset,
            guid,
            generation_count,
            generations,
        })
    }

    /// Serializes `FPackageFileSummary` to byte stream.
    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.tag.to_le_bytes());
        out.extend_from_slice(&self.file_version.to_le_bytes());
        out.extend_from_slice(&self.package_flags.to_le_bytes());
        out.extend_from_slice(&self.name_count.to_le_bytes());
        out.extend_from_slice(&self.name_offset.to_le_bytes());
        out.extend_from_slice(&self.export_count.to_le_bytes());
        out.extend_from_slice(&self.export_offset.to_le_bytes());
        out.extend_from_slice(&self.import_count.to_le_bytes());
        out.extend_from_slice(&self.import_offset.to_le_bytes());
        out.extend_from_slice(&self.guid);
        out.extend_from_slice(&(self.generations.len() as i32).to_le_bytes());
        for gen in &self.generations {
            out.extend_from_slice(&gen.export_count.to_le_bytes());
            out.extend_from_slice(&gen.name_count.to_le_bytes());
        }
    }
}
