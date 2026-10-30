use crate::error::PackageError;
use crate::export_table::{ExportTable, FObjectExport};
use crate::header::FPackageFileSummary;
use crate::import_table::{ImportTable, FObjectImport};
use crate::name_table::NameTable;
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;

/// Resolved reference to an object in an Unreal package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectRef<'a> {
    /// Null reference (index 0).
    Null,
    /// Reference to an external object in the Import Table.
    Import(&'a FObjectImport),
    /// Reference to an internal object in the Export Table.
    Export(&'a FObjectExport),
}

/// Zero-copy reader for an Unreal Package file V128 / Licensee 29 (`0x9E2A83C2`).
///
/// Borrows from underlying byte slice `&'a [u8]` without copying payloads.
#[derive(Debug, Clone)]
pub struct UnrealPackage<'a> {
    pub data: &'a [u8],
    pub summary: FPackageFileSummary,
    pub names: NameTable,
    pub imports: ImportTable,
    pub exports: ExportTable,
}

impl<'a> UnrealPackage<'a> {
    /// Parses an Unreal package from a borrowed byte slice.
    pub fn from_bytes(data: &'a [u8]) -> Result<Self, PackageError> {
        let mut offset = 0;
        let summary = FPackageFileSummary::read(data, &mut offset)?;

        let names = NameTable::read(
            data,
            summary.name_offset as usize,
            summary.name_count as usize,
        )?;

        let imports = ImportTable::read(
            data,
            summary.import_offset as usize,
            summary.import_count as usize,
        )?;

        let exports = ExportTable::read(
            data,
            summary.export_offset as usize,
            summary.export_count as usize,
        )?;

        Ok(Self {
            data,
            summary,
            names,
            imports,
            exports,
        })
    }

    /// Access the underlying raw byte slice.
    #[inline]
    pub fn raw_bytes(&self) -> &'a [u8] {
        self.data
    }

    /// Resolves a string from the Name Table by signed compact index.
    pub fn resolve_name(&self, index: i32) -> Result<&str, PackageError> {
        if index < 0 || index as usize >= self.names.len() {
            Err(PackageError::NameIndexOutOfBounds(
                index as usize,
                self.names.len(),
            ))
        } else {
            Ok(self.names.get(index as usize).unwrap_or(""))
        }
    }

    /// Resolves an object index into an `ObjectRef`.
    /// - `0`: Null / None.
    /// - `< 0`: Import entry at index `-index - 1`.
    /// - `> 0`: Export entry at index `index - 1`.
    pub fn resolve_object_ref(&self, index: i32) -> Result<ObjectRef<'_>, PackageError> {
        if index == 0 {
            Ok(ObjectRef::Null)
        } else if index < 0 {
            let imp_idx = (-index - 1) as usize;
            self.imports
                .get(imp_idx)
                .map(ObjectRef::Import)
                .ok_or(PackageError::ImportIndexOutOfBounds(imp_idx, self.imports.len()))
        } else {
            let exp_idx = (index - 1) as usize;
            self.exports
                .get(exp_idx)
                .map(ObjectRef::Export)
                .ok_or(PackageError::ExportIndexOutOfBounds(exp_idx, self.exports.len()))
        }
    }

    /// Formats the fully qualified name of an object reference (e.g. `Package.Class.Object`).
    pub fn format_full_name(&self, index: i32) -> String {
        // Iterative with cycle detection to prevent stack overflow on malicious packages
        let mut parts: Vec<String> = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut cur = index;
        for _ in 0..32 {
            if !visited.insert(cur) {
                parts.push("?".to_string());
                break;
            }
            match self.resolve_object_ref(cur) {
                Ok(ObjectRef::Null) => {
                    parts.push("None".to_string());
                    break;
                }
                Ok(ObjectRef::Import(imp)) => {
                    parts.push(imp.resolve_object_name(&self.names).unwrap_or("?").to_string());
                    if imp.package != 0 {
                        cur = imp.package;
                    } else {
                        break;
                    }
                }
                Ok(ObjectRef::Export(exp)) => {
                    parts.push(exp.resolve_object_name(&self.names).unwrap_or("?").to_string());
                    if exp.package != 0 {
                        cur = exp.package;
                    } else {
                        break;
                    }
                }
                Err(_) => {
                    parts.push("?".to_string());
                    break;
                }
            }
        }
        if parts.is_empty() {
            return "?".to_string();
        }
        parts.reverse();
        parts.join(".")
    }

    /// Zero-copy borrows the serialized raw payload of an exported object.
    pub fn get_export_slice(&self, export: &FObjectExport) -> Result<&'a [u8], PackageError> {
        if export.serial_size <= 0 {
            return Ok(&[]);
        }

        let start = export.serial_offset as usize;
        let end = start + export.serial_size as usize;

        if end > self.data.len() {
            return Err(PackageError::PayloadOutOfBounds {
                offset: start,
                size: export.serial_size as usize,
                total: self.data.len(),
            });
        }

        Ok(&self.data[start..end])
    }

    /// Zero-copy borrows the serialized raw payload of an export by 0-based index.
    pub fn get_export_data(&self, index: usize) -> Result<&'a [u8], PackageError> {
        let export = self
            .exports
            .get(index)
            .ok_or(PackageError::ExportIndexOutOfBounds(index, self.exports.len()))?;
        self.get_export_slice(export)
    }
}

/// Owned memory-mapped Unreal package reader.
pub struct MmapPackage {
    mmap: Mmap,
    pub summary: FPackageFileSummary,
    pub names: NameTable,
    pub imports: ImportTable,
    pub exports: ExportTable,
}

impl MmapPackage {
    /// Opens and memory-maps an Unreal package file from the filesystem.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PackageError> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        let mut offset = 0;
        let summary = FPackageFileSummary::read(&mmap, &mut offset)?;

        let names = NameTable::read(
            &mmap,
            summary.name_offset as usize,
            summary.name_count as usize,
        )?;

        let imports = ImportTable::read(
            &mmap,
            summary.import_offset as usize,
            summary.import_count as usize,
        )?;

        let exports = ExportTable::read(
            &mmap,
            summary.export_offset as usize,
            summary.export_count as usize,
        )?;

        Ok(Self {
            mmap,
            summary,
            names,
            imports,
            exports,
        })
    }

    /// Creates a borrowed zero-copy view of the package.
    pub fn as_package(&self) -> UnrealPackage<'_> {
        UnrealPackage {
            data: &self.mmap,
            summary: self.summary.clone(),
            names: self.names.clone(),
            imports: self.imports.clone(),
            exports: self.exports.clone(),
        }
    }

    /// Returns a zero-copy slice to the exported object's payload in memory.
    pub fn get_export_slice(&self, export: &FObjectExport) -> Result<&[u8], PackageError> {
        if export.serial_size <= 0 {
            return Ok(&[]);
        }

        let start = export.serial_offset as usize;
        let end = start + export.serial_size as usize;

        if end > self.mmap.len() {
            return Err(PackageError::PayloadOutOfBounds {
                offset: start,
                size: export.serial_size as usize,
                total: self.mmap.len(),
            });
        }

        Ok(&self.mmap[start..end])
    }

    /// Returns a zero-copy slice to the exported object's payload by index.
    pub fn get_export_data(&self, index: usize) -> Result<&[u8], PackageError> {
        let export = self
            .exports
            .get(index)
            .ok_or(PackageError::ExportIndexOutOfBounds(index, self.exports.len()))?;
        self.get_export_slice(export)
    }

    /// Resolves a string from the Name Table.
    pub fn resolve_name(&self, index: i32) -> Result<&str, PackageError> {
        if index < 0 || index as usize >= self.names.len() {
            Err(PackageError::NameIndexOutOfBounds(
                index as usize,
                self.names.len(),
            ))
        } else {
            Ok(self.names.get(index as usize).unwrap_or(""))
        }
    }

    /// Resolves an object index into an `ObjectRef`.
    pub fn resolve_object_ref(&self, index: i32) -> Result<ObjectRef<'_>, PackageError> {
        if index == 0 {
            Ok(ObjectRef::Null)
        } else if index < 0 {
            let imp_idx = (-index - 1) as usize;
            self.imports
                .get(imp_idx)
                .map(ObjectRef::Import)
                .ok_or(PackageError::ImportIndexOutOfBounds(imp_idx, self.imports.len()))
        } else {
            let exp_idx = (index - 1) as usize;
            self.exports
                .get(exp_idx)
                .map(ObjectRef::Export)
                .ok_or(PackageError::ExportIndexOutOfBounds(exp_idx, self.exports.len()))
        }
    }

    /// Formats the fully qualified name of an object reference.
    pub fn format_full_name(&self, index: i32) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut cur = index;
        for _ in 0..32 {
            if !visited.insert(cur) {
                parts.push("?".to_string());
                break;
            }
            match self.resolve_object_ref(cur) {
                Ok(ObjectRef::Null) => {
                    parts.push("None".to_string());
                    break;
                }
                Ok(ObjectRef::Import(imp)) => {
                    parts.push(imp.resolve_object_name(&self.names).unwrap_or("?").to_string());
                    if imp.package != 0 {
                        cur = imp.package;
                    } else {
                        break;
                    }
                }
                Ok(ObjectRef::Export(exp)) => {
                    parts.push(exp.resolve_object_name(&self.names).unwrap_or("?").to_string());
                    if exp.package != 0 {
                        cur = exp.package;
                    } else {
                        break;
                    }
                }
                Err(_) => {
                    parts.push("?".to_string());
                    break;
                }
            }
        }
        if parts.is_empty() {
            return "?".to_string();
        }
        parts.reverse();
        parts.join(".")
    }
}
