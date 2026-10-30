use crate::compact_index::{read_compact_index, write_compact_index};
use crate::error::PackageError;
use crate::name_table::NameTable;
use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct ObjectFlags: u32 {
        const STANDALONE      = 0x00000004;
        const PUBLIC          = 0x00010000;
        const LOAD_FOR_CLIENT = 0x00020000;
        const LOAD_FOR_SERVER = 0x00040000;
        const LOAD_FOR_EDIT   = 0x00080000;
    }
}

/// A record in the Export Table describing an asset or class instance inside this package (`FObjectExport`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FObjectExport {
    /// Object's class: 0 = `UClass`, < 0 = Import index (`-class_index - 1`), > 0 = Export index (`class_index - 1`).
    pub class_index: i32,
    /// Superclass index (0 = None, < 0 = Import index, > 0 = Export index).
    pub super_index: i32,
    /// Outer package reference (0 = root, < 0 = Import index, > 0 = Export index).
    pub package: i32,
    /// Index into `Names` table for this object's name.
    pub object_name: i32,
    /// Object lifecycle and persistence flags.
    pub object_flags: u32,
    /// Byte size of the object's serialized payload.
    pub serial_size: i32,
    /// Absolute file offset to the serialized payload.
    pub serial_offset: i32,
}

impl FObjectExport {
    pub fn new(
        class_index: i32,
        super_index: i32,
        package: i32,
        object_name: i32,
        object_flags: u32,
        serial_size: i32,
        serial_offset: i32,
    ) -> Self {
        Self {
            class_index,
            super_index,
            package,
            object_name,
            object_flags,
            serial_size,
            serial_offset,
        }
    }

    pub fn resolve_object_name<'a>(&self, names: &'a NameTable) -> Option<&'a str> {
        if self.object_name >= 0 {
            names.get(self.object_name as usize)
        } else {
            None
        }
    }

    /// Returns true if this export is a `UClass` definition itself.
    #[inline]
    pub fn is_class_definition(&self) -> bool {
        self.class_index == 0
    }

    pub fn class_import_index(&self) -> Option<usize> {
        if self.class_index < 0 {
            Some((-self.class_index - 1) as usize)
        } else {
            None
        }
    }

    pub fn class_export_index(&self) -> Option<usize> {
        if self.class_index > 0 {
            Some((self.class_index - 1) as usize)
        } else {
            None
        }
    }

    pub fn super_import_index(&self) -> Option<usize> {
        if self.super_index < 0 {
            Some((-self.super_index - 1) as usize)
        } else {
            None
        }
    }

    pub fn super_export_index(&self) -> Option<usize> {
        if self.super_index > 0 {
            Some((self.super_index - 1) as usize)
        } else {
            None
        }
    }

    pub fn outer_import_index(&self) -> Option<usize> {
        if self.package < 0 {
            Some((-self.package - 1) as usize)
        } else {
            None
        }
    }

    pub fn outer_export_index(&self) -> Option<usize> {
        if self.package > 0 {
            Some((self.package - 1) as usize)
        } else {
            None
        }
    }
}

/// Export Table containing all objects and resources stored in this package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportTable {
    pub entries: Vec<FObjectExport>,
}

impl ExportTable {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&FObjectExport> {
        self.entries.get(index)
    }

    pub fn add(&mut self, export: FObjectExport) -> usize {
        let idx = self.entries.len();
        self.entries.push(export);
        idx
    }

    /// Deserializes the Export Table from byte slice starting at `offset`.
    pub fn read(data: &[u8], offset: usize, count: usize) -> Result<Self, PackageError> {
        let mut curr = offset;
        let mut table = Self::with_capacity(count);

        for _ in 0..count {
            let class_index = read_compact_index(data, &mut curr)?;
            let super_index = read_compact_index(data, &mut curr)?;

            if curr + 4 > data.len() {
                return Err(PackageError::UnexpectedEof {
                    offset: curr,
                    requested: 4,
                    total: data.len(),
                });
            }
            let package = i32::from_le_bytes(data[curr..curr + 4].try_into().unwrap());
            curr += 4;

            let object_name = read_compact_index(data, &mut curr)?;

            if curr + 4 > data.len() {
                return Err(PackageError::UnexpectedEof {
                    offset: curr,
                    requested: 4,
                    total: data.len(),
                });
            }
            let object_flags = u32::from_le_bytes(data[curr..curr + 4].try_into().unwrap());
            curr += 4;

            let serial_size = read_compact_index(data, &mut curr)?;
            let serial_offset = if serial_size > 0 {
                read_compact_index(data, &mut curr)?
            } else {
                0
            };

            table.entries.push(FObjectExport {
                class_index,
                super_index,
                package,
                object_name,
                object_flags,
                serial_size,
                serial_offset,
            });
        }

        Ok(table)
    }

    /// Serializes the Export Table to byte stream.
    pub fn write(&self, out: &mut Vec<u8>) {
        for entry in &self.entries {
            write_compact_index(entry.class_index, out);
            write_compact_index(entry.super_index, out);
            out.extend_from_slice(&entry.package.to_le_bytes());
            write_compact_index(entry.object_name, out);
            out.extend_from_slice(&entry.object_flags.to_le_bytes());
            write_compact_index(entry.serial_size, out);
            if entry.serial_size > 0 {
                write_compact_index(entry.serial_offset, out);
            }
        }
    }
}
