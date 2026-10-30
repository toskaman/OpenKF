use crate::compact_index::{read_compact_index, write_compact_index};
use crate::error::PackageError;
use crate::name_table::NameTable;

/// A record in the Import Table describing an external dependency (`FObjectImport`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FObjectImport {
    /// Index into `Names` table for the package containing this object's class.
    pub class_package: i32,
    /// Index into `Names` table for the class name.
    pub class_name: i32,
    /// Outer package reference (0 = root, < 0 = Import index, > 0 = Export index).
    pub package: i32,
    /// Index into `Names` table for this object's name.
    pub object_name: i32,
}

impl FObjectImport {
    pub fn new(class_package: i32, class_name: i32, package: i32, object_name: i32) -> Self {
        Self {
            class_package,
            class_name,
            package,
            object_name,
        }
    }

    pub fn resolve_object_name<'a>(&self, names: &'a NameTable) -> Option<&'a str> {
        if self.object_name >= 0 {
            names.get(self.object_name as usize)
        } else {
            None
        }
    }

    pub fn resolve_class_name<'a>(&self, names: &'a NameTable) -> Option<&'a str> {
        if self.class_name >= 0 {
            names.get(self.class_name as usize)
        } else {
            None
        }
    }

    pub fn resolve_class_package<'a>(&self, names: &'a NameTable) -> Option<&'a str> {
        if self.class_package >= 0 {
            names.get(self.class_package as usize)
        } else {
            None
        }
    }

    /// If this object's outer is another import, returns its 0-based import index.
    pub fn outer_import_index(&self) -> Option<usize> {
        if self.package < 0 {
            Some((-self.package - 1) as usize)
        } else {
            None
        }
    }

    /// If this object's outer is an export, returns its 0-based export index.
    pub fn outer_export_index(&self) -> Option<usize> {
        if self.package > 0 {
            Some((self.package - 1) as usize)
        } else {
            None
        }
    }
}

/// Import Table containing all external class and package references.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportTable {
    pub entries: Vec<FObjectImport>,
}

impl ImportTable {
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

    pub fn get(&self, index: usize) -> Option<&FObjectImport> {
        self.entries.get(index)
    }

    pub fn add(&mut self, import: FObjectImport) -> usize {
        let idx = self.entries.len();
        self.entries.push(import);
        idx
    }

    /// Deserializes the Import Table from byte slice starting at `offset`.
    pub fn read(data: &[u8], offset: usize, count: usize) -> Result<Self, PackageError> {
        let mut curr = offset;
        let mut table = Self::with_capacity(count);

        for _ in 0..count {
            let class_pkg = read_compact_index(data, &mut curr)?;
            let class_name = read_compact_index(data, &mut curr)?;

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

            table.entries.push(FObjectImport {
                class_package: class_pkg,
                class_name,
                package,
                object_name,
            });
        }

        Ok(table)
    }

    /// Serializes the Import Table to byte stream.
    pub fn write(&self, out: &mut Vec<u8>) {
        for entry in &self.entries {
            write_compact_index(entry.class_package, out);
            write_compact_index(entry.class_name, out);
            out.extend_from_slice(&entry.package.to_le_bytes());
            write_compact_index(entry.object_name, out);
        }
    }
}
