use bincode::Options;
use serde::Deserialize;
use std::fmt::Write;

pub fn decode_bytes_be<'de, T>(bytes: &'de [u8]) -> T where T: Deserialize<'de> {
    let config = bincode::DefaultOptions::new()
        .with_big_endian()
        .allow_trailing_bytes()
        .with_fixint_encoding();
    config.deserialize::<T>(bytes).expect("Parsed struct")
}

pub fn format_hex(buffer: &[u8]) -> String {
    let mut s = String::new();
    write!(s, "0x").unwrap();
    for i in 0..buffer.len() {
        write!(s, "{:02x}", buffer[i]).unwrap();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    #[derive(Deserialize)]
    struct TestStruct {
        signature: [u8; 8],
        version: u32,
        length: u32,
        checksum: u8,
    }

    #[test]
    fn decoding() {
        let car = vec![
            0, 1, 2, 3, 4, 5, 6, 7,
            0, 0, 0, 1,
            0, 0, 0, 2,
            3,
        ];
        let s = decode_bytes_be::<TestStruct>(&car);
        assert_eq!(s.signature, [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(s.version, 1);
        assert_eq!(s.length, 2);
        assert_eq!(s.checksum, 3);
    }

    #[test]
    fn formatting_hex() {
        let car = vec![
            0, 1, 2, 3, 4, 5, 6, 7,
            0, 0, 0, 1,
            0, 0, 0, 2,
            3,
        ];
        let s = format_hex(&car);
        assert_eq!(s, "0x0001020304050607000000010000000203");
    }
}