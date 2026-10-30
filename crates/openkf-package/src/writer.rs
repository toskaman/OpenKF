use crate::export_table::{ExportTable, FObjectExport};
use crate::header::{
    FGenerationInfo, FPackageFileSummary, ENGINE_VERSION_KF, LICENSEE_VERSION_KF, PACKAGE_TAG,
};
use crate::import_table::{ImportTable, FObjectImport};
use crate::name_table::NameTable;

/// Builder and serializer for Unreal Package V128 / Licensee 29 files.
#[derive(Debug, Clone, Default)]
pub struct PackageWriter {
    pub package_flags: u32,
    pub guid: [u8; 16],
    pub names: NameTable,
    pub imports: ImportTable,
    pub exports: ExportTable,
    pub payloads: Vec<Vec<u8>>,
}

impl PackageWriter {
    pub fn new() -> Self {
        Self {
            package_flags: 0,
            guid: [0u8; 16],
            names: NameTable::new(),
            imports: ImportTable::new(),
            exports: ExportTable::new(),
            payloads: Vec::new(),
        }
    }

    pub fn with_guid(mut self, guid: [u8; 16]) -> Self {
        self.guid = guid;
        self
    }

    pub fn with_package_flags(mut self, flags: u32) -> Self {
        self.package_flags = flags;
        self
    }

    pub fn add_name(&mut self, name: impl Into<String>, flags: u32) -> usize {
        self.names.add(name, flags)
    }

    pub fn add_import(&mut self, import: FObjectImport) -> usize {
        self.imports.add(import)
    }

    /// Adds an export with its binary payload.
    ///
    /// Automatically records `payload.len()` in `export.serial_size`.
    /// The `serial_offset` will be computed during `build()`.
    pub fn add_export(&mut self, mut export: FObjectExport, payload: Vec<u8>) -> usize {
        export.serial_size = payload.len() as i32;
        self.payloads.push(payload);
        self.exports.add(export)
    }

    /// Compiles and serializes the complete Unreal package into a binary buffer.
    pub fn build(mut self) -> Vec<u8> {
        // Step 1: Write Names table to intermediate buffer
        let mut names_buf = Vec::new();
        self.names.write(&mut names_buf);

        // Step 2: Determine header size (56 bytes + 1 generation of 8 bytes = 64 bytes)
        let header_size = 56 + 8;
        let name_offset = header_size as i32;
        let name_count = self.names.len() as i32;

        // Step 3: Compute payload offsets
        let payloads_start = header_size + names_buf.len();
        let mut curr_payload_offset = payloads_start;
        for (i, payload) in self.payloads.iter().enumerate() {
            if let Some(exp) = self.exports.entries.get_mut(i) {
                if !payload.is_empty() {
                    exp.serial_size = payload.len() as i32;
                    exp.serial_offset = curr_payload_offset as i32;
                    curr_payload_offset += payload.len();
                } else {
                    exp.serial_size = 0;
                    exp.serial_offset = 0;
                }
            }
        }

        // Step 4: Write Imports table to intermediate buffer
        let import_offset = curr_payload_offset as i32;
        let import_count = self.imports.len() as i32;
        let mut imports_buf = Vec::new();
        self.imports.write(&mut imports_buf);

        // Step 5: Write Exports table to intermediate buffer
        let export_offset = (curr_payload_offset + imports_buf.len()) as i32;
        let export_count = self.exports.len() as i32;
        let mut exports_buf = Vec::new();
        self.exports.write(&mut exports_buf);

        // Step 6: Construct Summary Header
        let file_version = (ENGINE_VERSION_KF as i32) | ((LICENSEE_VERSION_KF as i32) << 16);
        let summary = FPackageFileSummary {
            tag: PACKAGE_TAG,
            file_version,
            package_flags: self.package_flags,
            name_count,
            name_offset,
            export_count,
            export_offset,
            import_count,
            import_offset,
            guid: self.guid,
            generation_count: 1,
            generations: vec![FGenerationInfo {
                export_count,
                name_count,
            }],
        };

        // Step 7: Assemble final output
        let mut out = Vec::with_capacity(
            header_size
                + names_buf.len()
                + (curr_payload_offset - payloads_start)
                + imports_buf.len()
                + exports_buf.len(),
        );

        summary.write(&mut out);
        out.extend_from_slice(&names_buf);

        for payload in &self.payloads {
            out.extend_from_slice(payload);
        }

        out.extend_from_slice(&imports_buf);
        out.extend_from_slice(&exports_buf);

        out
    }
}
