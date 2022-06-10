use std::fmt::Write;

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