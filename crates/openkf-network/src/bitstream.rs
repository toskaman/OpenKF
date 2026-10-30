use crate::error::{NetworkError, NetworkResult};
use glam::Vec3;

/// Bit-level stream reader for parsing Unreal Engine 2.5 network packets and bunches.
/// Implements `UFBitReader` semantics: reading single bits, bounded integers, packed integers,
/// IEEE 754 floats, strings, and spatial vectors.
#[derive(Debug, Clone)]
pub struct BitReader<'a> {
    buffer: &'a [u8],
    bit_position: usize,
    total_bits: usize,
}

/// Type alias matching retail Unreal Engine naming.
pub type UFBitReader<'a> = BitReader<'a>;

impl<'a> BitReader<'a> {
    /// Constructs a new `BitReader` from a byte slice.
    /// Total readable bits defaults to `buffer.len() * 8`.
    pub fn new(buffer: &'a [u8]) -> Self {
        let total_bits = buffer.len() * 8;
        Self {
            buffer,
            bit_position: 0,
            total_bits,
        }
    }

    /// Constructs a `BitReader` with an explicit bit limit.
    pub fn with_bit_count(buffer: &'a [u8], total_bits: usize) -> Self {
        let max_possible = buffer.len() * 8;
        let total_bits = total_bits.min(max_possible);
        Self {
            buffer,
            bit_position: 0,
            total_bits,
        }
    }

    /// Current bit cursor position.
    #[inline]
    pub fn bit_position(&self) -> usize {
        self.bit_position
    }

    /// Total number of bits in this stream.
    #[inline]
    pub fn total_bits(&self) -> usize {
        self.total_bits
    }

    /// Number of remaining bits available to read.
    #[inline]
    pub fn remaining_bits(&self) -> usize {
        self.total_bits.saturating_sub(self.bit_position)
    }

    /// Returns `true` if the reader has reached or passed the end of the bitstream.
    #[inline]
    pub fn is_at_end(&self) -> bool {
        self.bit_position >= self.total_bits
    }

    /// Sets the current bit position within valid bounds.
    pub fn set_bit_position(&mut self, pos: usize) -> NetworkResult<()> {
        if pos <= self.total_bits {
            self.bit_position = pos;
            Ok(())
        } else {
            Err(NetworkError::BitstreamOverflow {
                max_bits: self.total_bits,
            })
        }
    }

    /// Reads a single bit from the stream.
    pub fn read_bit(&mut self) -> NetworkResult<bool> {
        if self.bit_position >= self.total_bits {
            return Err(NetworkError::BitstreamUnderflow {
                needed_bits: 1,
                available_bits: self.remaining_bits(),
            });
        }

        let byte_index = self.bit_position / 8;
        let bit_index = self.bit_position % 8;
        self.bit_position += 1;

        let bit = (self.buffer[byte_index] >> bit_index) & 1;
        Ok(bit != 0)
    }

    /// Reads up to 64 bits from the stream, returned in little-endian order.
    pub fn read_bits(&mut self, num_bits: usize) -> NetworkResult<u64> {
        if num_bits > 64 {
            return Err(NetworkError::BitstreamOverflow { max_bits: 64 });
        }
        if self.remaining_bits() < num_bits {
            return Err(NetworkError::BitstreamUnderflow {
                needed_bits: num_bits,
                available_bits: self.remaining_bits(),
            });
        }

        let mut result: u64 = 0;
        for i in 0..num_bits {
            let byte_index = self.bit_position / 8;
            let bit_index = self.bit_position % 8;
            let bit = ((self.buffer[byte_index] >> bit_index) & 1) as u64;
            result |= bit << i;
            self.bit_position += 1;
        }

        Ok(result)
    }

    /// Reads an 8-bit unsigned integer.
    #[inline]
    pub fn read_u8(&mut self) -> NetworkResult<u8> {
        self.read_bits(8).map(|v| v as u8)
    }

    /// Reads a 16-bit unsigned integer (little-endian).
    #[inline]
    pub fn read_u16(&mut self) -> NetworkResult<u16> {
        self.read_bits(16).map(|v| v as u16)
    }

    /// Reads a 32-bit unsigned integer (little-endian).
    #[inline]
    pub fn read_u32(&mut self) -> NetworkResult<u32> {
        self.read_bits(32).map(|v| v as u32)
    }

    /// Reads a 32-bit signed integer (little-endian).
    #[inline]
    pub fn read_i32(&mut self) -> NetworkResult<i32> {
        self.read_bits(32).map(|v| v as i32)
    }

    /// Reads a 64-bit unsigned integer (little-endian).
    #[inline]
    pub fn read_u64(&mut self) -> NetworkResult<u64> {
        self.read_bits(64)
    }

    /// Reads a 64-bit signed integer (little-endian).
    #[inline]
    pub fn read_i64(&mut self) -> NetworkResult<i64> {
        self.read_bits(64).map(|v| v as i64)
    }

    /// Reads a 32-bit IEEE 754 floating-point number.
    pub fn read_f32(&mut self) -> NetworkResult<f32> {
        let bits = self.read_u32()?;
        Ok(f32::from_bits(bits))
    }

    /// Reads a 64-bit IEEE 754 floating-point number.
    pub fn read_f64(&mut self) -> NetworkResult<f64> {
        let bits = self.read_u64()?;
        Ok(f64::from_bits(bits))
    }

    /// Reads an integer bounded by `max_value` using the minimal number of bits required
    /// (`ceil(log2(max_value))`), implementing Unreal's `SerializeInt`.
    pub fn read_int(&mut self, max_value: u32) -> NetworkResult<u32> {
        if max_value <= 1 {
            return Ok(0);
        }
        let bits_needed = calculate_bits_needed(max_value);
        let val = self.read_bits(bits_needed)? as u32;
        Ok(val)
    }

    /// Reads a packed 32-bit integer using 7-bit chunks with continuation flag (UE `SerializeIntPacked`).
    pub fn read_compact_u32(&mut self) -> NetworkResult<u32> {
        let mut value: u32 = 0;
        let mut shift = 0;

        loop {
            let byte = self.read_u8()?;
            value |= ((byte & 0x7F) as u32) << shift;
            if (byte & 0x80) == 0 {
                break;
            }
            shift += 7;
            if shift >= 35 {
                return Err(NetworkError::MalformedBunch(
                    "Compact integer overflow in bitstream".to_string(),
                ));
            }
        }

        Ok(value)
    }

    /// Reads raw bytes into the provided slice.
    pub fn read_bytes(&mut self, dest: &mut [u8]) -> NetworkResult<()> {
        for byte in dest.iter_mut() {
            *byte = self.read_u8()?;
        }
        Ok(())
    }

    /// Reads a vector of bytes of the specified length.
    pub fn read_byte_vec(&mut self, length: usize) -> NetworkResult<Vec<u8>> {
        let mut vec = vec![0u8; length];
        self.read_bytes(&mut vec)?;
        Ok(vec)
    }

    /// Reads an Unreal-style length-prefixed string.
    pub fn read_string(&mut self) -> NetworkResult<String> {
        let length = self.read_compact_u32()? as usize;
        if length == 0 {
            return Ok(String::new());
        }

        let bytes = self.read_byte_vec(length)?;
        // Strip trailing null terminator if present
        let slice = if bytes.ends_with(&[0]) {
            &bytes[..bytes.len() - 1]
        } else {
            &bytes[..]
        };

        String::from_utf8(slice.to_vec())
            .map_err(|e| NetworkError::MalformedBunch(format!("Invalid UTF-8 string: {e}")))
    }

    /// Reads a 3D vector (X, Y, Z as 32-bit floats).
    pub fn read_vector(&mut self) -> NetworkResult<Vec3> {
        let x = self.read_f32()?;
        let y = self.read_f32()?;
        let z = self.read_f32()?;
        Ok(Vec3::new(x, y, z))
    }

    /// Reads an Unreal Rotator (Pitch, Yaw, Roll as 32-bit integers).
    pub fn read_rotator(&mut self) -> NetworkResult<(i32, i32, i32)> {
        let pitch = self.read_i32()?;
        let yaw = self.read_i32()?;
        let roll = self.read_i32()?;
        Ok((pitch, yaw, roll))
    }
}

/// Bit-level stream writer for assembling Unreal Engine 2.5 network packets and bunches.
/// Implements `UFBitWriter` semantics.
#[derive(Debug, Clone, Default)]
pub struct BitWriter {
    buffer: Vec<u8>,
    bit_position: usize,
}

/// Type alias matching retail Unreal Engine naming.
pub type UFBitWriter = BitWriter;

impl BitWriter {
    /// Creates a new, empty `BitWriter`.
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            bit_position: 0,
        }
    }

    /// Creates a new `BitWriter` with a pre-allocated byte capacity.
    pub fn with_capacity(byte_capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(byte_capacity),
            bit_position: 0,
        }
    }

    /// Current bit cursor position / total bits written.
    #[inline]
    pub fn num_bits(&self) -> usize {
        self.bit_position
    }

    /// Total number of bytes used (rounded up to nearest whole byte).
    #[inline]
    pub fn num_bytes(&self) -> usize {
        self.bit_position.div_ceil(8)
    }

    /// Returns a slice of the internal buffer containing all written bytes.
    pub fn data(&self) -> &[u8] {
        let bytes_len = self.num_bytes();
        if bytes_len <= self.buffer.len() {
            &self.buffer[..bytes_len]
        } else {
            &self.buffer
        }
    }

    /// Consumes the writer and returns the compiled byte buffer.
    pub fn into_bytes(mut self) -> Vec<u8> {
        let bytes_len = self.num_bytes();
        self.buffer.truncate(bytes_len);
        self.buffer
    }

    /// Writes a single bit into the stream.
    pub fn write_bit(&mut self, bit: bool) {
        let byte_index = self.bit_position / 8;
        let bit_index = self.bit_position % 8;

        if byte_index >= self.buffer.len() {
            self.buffer.push(0);
        }

        if bit {
            self.buffer[byte_index] |= 1 << bit_index;
        } else {
            self.buffer[byte_index] &= !(1 << bit_index);
        }

        self.bit_position += 1;
    }

    /// Writes up to 64 bits into the stream in little-endian bit order.
    pub fn write_bits(&mut self, value: u64, num_bits: usize) {
        assert!(num_bits <= 64, "Cannot write more than 64 bits at once");
        for i in 0..num_bits {
            let bit = ((value >> i) & 1) != 0;
            self.write_bit(bit);
        }
    }

    /// Writes an 8-bit unsigned integer.
    #[inline]
    pub fn write_u8(&mut self, value: u8) {
        self.write_bits(value as u64, 8);
    }

    /// Writes a 16-bit unsigned integer (little-endian).
    #[inline]
    pub fn write_u16(&mut self, value: u16) {
        self.write_bits(value as u64, 16);
    }

    /// Writes a 32-bit unsigned integer (little-endian).
    #[inline]
    pub fn write_u32(&mut self, value: u32) {
        self.write_bits(value as u64, 32);
    }

    /// Writes a 32-bit signed integer (little-endian).
    #[inline]
    pub fn write_i32(&mut self, value: i32) {
        self.write_bits(value as u32 as u64, 32);
    }

    /// Writes a 64-bit unsigned integer (little-endian).
    #[inline]
    pub fn write_u64(&mut self, value: u64) {
        self.write_bits(value, 64);
    }

    /// Writes a 64-bit signed integer (little-endian).
    #[inline]
    pub fn write_i64(&mut self, value: i64) {
        self.write_bits(value as u64, 64);
    }

    /// Writes a 32-bit IEEE 754 floating-point number.
    #[inline]
    pub fn write_f32(&mut self, value: f32) {
        self.write_u32(value.to_bits());
    }

    /// Writes a 64-bit IEEE 754 floating-point number.
    #[inline]
    pub fn write_f64(&mut self, value: f64) {
        self.write_u64(value.to_bits());
    }

    /// Writes an integer bounded by `max_value` using the minimal number of bits required
    /// (`ceil(log2(max_value))`), implementing Unreal's `SerializeInt`.
    pub fn write_int(&mut self, value: u32, max_value: u32) {
        if max_value <= 1 {
            return;
        }
        let bits_needed = calculate_bits_needed(max_value);
        self.write_bits(value as u64, bits_needed);
    }

    /// Writes a packed 32-bit integer using 7-bit chunks with continuation flag (UE `SerializeIntPacked`).
    pub fn write_compact_u32(&mut self, mut value: u32) {
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value > 0 {
                byte |= 0x80;
                self.write_u8(byte);
            } else {
                self.write_u8(byte);
                break;
            }
        }
    }

    /// Writes a raw slice of bytes.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u8(byte);
        }
    }

    /// Writes an Unreal-style length-prefixed string.
    pub fn write_string(&mut self, s: &str) {
        let bytes = s.as_bytes();
        // Write byte length including null terminator
        let len_with_null = (bytes.len() + 1) as u32;
        self.write_compact_u32(len_with_null);
        self.write_bytes(bytes);
        self.write_u8(0); // Null terminator
    }

    /// Writes a 3D vector (X, Y, Z as 32-bit floats).
    pub fn write_vector(&mut self, v: Vec3) {
        self.write_f32(v.x);
        self.write_f32(v.y);
        self.write_f32(v.z);
    }

    /// Writes an Unreal Rotator (Pitch, Yaw, Roll as 32-bit integers).
    pub fn write_rotator(&mut self, pitch: i32, yaw: i32, roll: i32) {
        self.write_i32(pitch);
        self.write_i32(yaw);
        self.write_i32(roll);
    }

    /// Appends the bits from another `BitReader` into this writer.
    pub fn append_bits_from_reader(&mut self, reader: &mut BitReader) -> NetworkResult<()> {
        let count = reader.remaining_bits();
        for _ in 0..count {
            let bit = reader.read_bit()?;
            self.write_bit(bit);
        }
        Ok(())
    }
}

/// Helper function to calculate the minimum number of bits needed to represent values up to `max_value`.
/// Corresponds to `FMath::CeilLogTwo(max_value)`.
#[inline]
pub fn calculate_bits_needed(max_value: u32) -> usize {
    if max_value <= 1 {
        return 0;
    }
    let mut bits = 0;
    let mut mask = 1u32;
    while mask < max_value {
        bits += 1;
        mask <<= 1;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bits_needed() {
        assert_eq!(calculate_bits_needed(1), 0);
        assert_eq!(calculate_bits_needed(2), 1);
        assert_eq!(calculate_bits_needed(3), 2);
        assert_eq!(calculate_bits_needed(4), 2);
        assert_eq!(calculate_bits_needed(5), 3);
        assert_eq!(calculate_bits_needed(8), 3);
        assert_eq!(calculate_bits_needed(9), 4);
        assert_eq!(calculate_bits_needed(1024), 10);
    }

    #[test]
    fn test_bit_reader_writer_primitives() {
        let mut writer = BitWriter::new();
        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bits(0b1011, 4);
        writer.write_u8(0x42);
        writer.write_u16(0x1234);
        writer.write_u32(0xdeadbeef);
        writer.write_f32(123.456);
        writer.write_string("KillingFloor");
        writer.write_vector(Vec3::new(10.0, -20.5, 30.25));

        let data = writer.into_bytes();
        let mut reader = BitReader::new(&data);

        assert_eq!(reader.read_bit().unwrap(), true);
        assert_eq!(reader.read_bit().unwrap(), false);
        assert_eq!(reader.read_bit().unwrap(), true);
        assert_eq!(reader.read_bits(4).unwrap(), 0b1011);
        assert_eq!(reader.read_u8().unwrap(), 0x42);
        assert_eq!(reader.read_u16().unwrap(), 0x1234);
        assert_eq!(reader.read_u32().unwrap(), 0xdeadbeef);
        assert!((reader.read_f32().unwrap() - 123.456).abs() < 1e-4);
        assert_eq!(reader.read_string().unwrap(), "KillingFloor");
        let v = reader.read_vector().unwrap();
        assert!((v.x - 10.0).abs() < 1e-4);
        assert!((v.y - (-20.5)).abs() < 1e-4);
        assert!((v.z - 30.25).abs() < 1e-4);
    }

    #[test]
    fn test_compact_integer_roundtrip() {
        let values = [0, 1, 127, 128, 255, 300, 16384, 1000000];
        let mut writer = BitWriter::new();
        for &val in &values {
            writer.write_compact_u32(val);
        }

        let data = writer.into_bytes();
        let mut reader = BitReader::new(&data);
        for &val in &values {
            assert_eq!(reader.read_compact_u32().unwrap(), val);
        }
    }

    #[test]
    fn test_bounded_int_roundtrip() {
        let mut writer = BitWriter::new();
        writer.write_int(3, 8); // 3 bits
        writer.write_int(15, 16); // 4 bits
        writer.write_int(511, 1024); // 10 bits

        let data = writer.into_bytes();
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_int(8).unwrap(), 3);
        assert_eq!(reader.read_int(16).unwrap(), 15);
        assert_eq!(reader.read_int(1024).unwrap(), 511);
    }
}
