use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum PackageError {
    #[error("Invalid package magic tag: expected 0x9E2A83C2, got {0:#010X}")]
    InvalidTag(u32),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Unsupported package version: engine={engine}, licensee={licensee}")]
    UnsupportedVersion { engine: u16, licensee: u16 },

    #[error("Unexpected EOF while parsing package: offset {offset}, requested {requested}, total {total}")]
    UnexpectedEof {
        offset: usize,
        requested: usize,
        total: usize,
    },

    #[error("Malformed FCompactIndex at offset {0}")]
    InvalidCompactIndex(usize),

    #[error("Invalid name entry length {0} at offset {1}")]
    InvalidNameLength(i8, usize),

    #[error("Invalid UTF-8/ANSI string in name entry at offset {0}")]
    InvalidString(usize),

    #[error("Invalid UTF-16LE string in name entry at offset {0}")]
    InvalidUtf16String(usize),

    #[error("Name index {0} out of bounds (table size {1})")]
    NameIndexOutOfBounds(usize, usize),

    #[error("Import index {0} out of bounds (table size {1})")]
    ImportIndexOutOfBounds(usize, usize),

    #[error("Export index {0} out of bounds (table size {1})")]
    ExportIndexOutOfBounds(usize, usize),

    #[error("Export payload out of bounds: offset {offset}, size {size}, total {total}")]
    PayloadOutOfBounds {
        offset: usize,
        size: usize,
        total: usize,
    },

    #[error("I/O error: {0}")]
    Io(String),
}

impl From<std::io::Error> for PackageError {
    fn from(err: std::io::Error) -> Self {
        PackageError::Io(err.to_string())
    }
}
