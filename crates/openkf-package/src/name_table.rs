use crate::error::PackageError;

/// An entry in the Unreal package Name Table (`FNameEntry`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FNameEntry {
    pub name: String,
    pub flags: u32,
}

impl FNameEntry {
    pub fn new(name: impl Into<String>, flags: u32) -> Self {
        Self {
            name: name.into(),
            flags,
        }
    }
}

/// Name Table holding all unique identifiers and symbols for an Unreal package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameTable {
    pub entries: Vec<FNameEntry>,
}

impl NameTable {
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

    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|e| e.name.as_str())
    }

    pub fn get_entry(&self, index: usize) -> Option<&FNameEntry> {
        self.entries.get(index)
    }

    /// Case-insensitive search for an entry name.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Appends a new name entry and returns its index.
    pub fn add(&mut self, name: impl Into<String>, flags: u32) -> usize {
        let idx = self.entries.len();
        self.entries.push(FNameEntry::new(name, flags));
        idx
    }

    /// Deserializes the Name Table from byte slice starting at `offset`.
    pub fn read(data: &[u8], offset: usize, count: usize) -> Result<Self, PackageError> {
        let mut curr = offset;
        let mut table = Self::with_capacity(count);

        for _ in 0..count {
            if curr >= data.len() {
                return Err(PackageError::UnexpectedEof {
                    offset: curr,
                    requested: 1,
                    total: data.len(),
                });
            }

            // Length is an FCompactIndex (1 byte below 64, 2+ bytes above).
            // Positive = Latin-1 bytes, negative = UTF-16 code units; both include the null.
            let len_val = crate::compact_index::read_compact_index(data, &mut curr)?;

            let name = if len_val > 0 {
                let len = len_val as usize;
                if curr + len > data.len() {
                    return Err(PackageError::UnexpectedEof {
                        offset: curr,
                        requested: len,
                        total: data.len(),
                    });
                }
                let raw_bytes = &data[curr..curr + len];
                curr += len;

                // Strip trailing null byte if present
                let str_slice = if let Some(last) = raw_bytes.last() {
                    if *last == 0 {
                        &raw_bytes[..raw_bytes.len() - 1]
                    } else {
                        raw_bytes
                    }
                } else {
                    raw_bytes
                };

                // Decode as Latin1 / ASCII
                str_slice.iter().map(|&b| b as char).collect::<String>()
            } else if len_val < 0 {
                let char_count = len_val.unsigned_abs() as usize;
                let byte_len = char_count * 2;
                if curr + byte_len > data.len() {
                    return Err(PackageError::UnexpectedEof {
                        offset: curr,
                        requested: byte_len,
                        total: data.len(),
                    });
                }

                let raw_bytes = &data[curr..curr + byte_len];
                curr += byte_len;

                let mut u16_chars = Vec::with_capacity(char_count);
                for chunk in raw_bytes.chunks_exact(2) {
                    u16_chars.push(u16::from_le_bytes([chunk[0], chunk[1]]));
                }

                // Strip trailing null char
                if let Some(&0) = u16_chars.last() {
                    u16_chars.pop();
                }

                String::from_utf16_lossy(&u16_chars)
            } else {
                String::new()
            };

            if curr + 4 > data.len() {
                return Err(PackageError::UnexpectedEof {
                    offset: curr,
                    requested: 4,
                    total: data.len(),
                });
            }

            let flags = u32::from_le_bytes(data[curr..curr + 4].try_into().unwrap());
            curr += 4;

            table.entries.push(FNameEntry { name, flags });
        }

        Ok(table)
    }

    /// Serializes the Name Table to byte stream.
    pub fn write(&self, out: &mut Vec<u8>) {
        for entry in &self.entries {
            // Check if string contains any non-ASCII characters
            let is_ascii = entry.name.is_ascii();
            if is_ascii {
                // Serialized length includes trailing \0
                crate::compact_index::write_compact_index((entry.name.len() + 1) as i32, out);
                out.extend_from_slice(entry.name.as_bytes());
                out.push(0x00);
            } else {
                let utf16: Vec<u16> = entry.name.encode_utf16().chain(std::iter::once(0)).collect();
                crate::compact_index::write_compact_index(-(utf16.len() as i32), out);
                for u in utf16 {
                    out.extend_from_slice(&u.to_le_bytes());
                }
            }
            out.extend_from_slice(&entry.flags.to_le_bytes());
        }
    }
}
