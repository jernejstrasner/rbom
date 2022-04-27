use std::fmt::Write;
use bytes::Buf;
use std::string::FromUtf8Error;

pub fn format_hex(buffer: &[u8]) -> String {
    let mut s = String::new();
    write!(s, "0x").unwrap();
    for i in 0..buffer.len() {
        write!(s, "{:02x}", buffer[i]).unwrap();
    }
    s
}

pub trait GetString: Buf {
    fn get_string(&mut self, length: usize) -> Result<String, FromUtf8Error>;
}

impl<B: Buf> GetString for B {
    fn get_string(&mut self, length: usize) -> Result<String, FromUtf8Error> {
        let mut buf = vec![0; length];
        self.copy_to_slice(&mut buf);
        String::from_utf8(buf)
    }
}

pub trait GetBytes: Buf {
    fn get_bytes<const N: usize>(&mut self) -> [u8; N];
}

impl<B: Buf> GetBytes for B {
    fn get_bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut buf = [0; N];
        self.copy_to_slice(&mut buf);
        buf
    }
}

pub trait GetStruct<T>: Buf {
    fn get_struct(&mut self) -> T;
}

impl<B: Buf + Clone, T: From<B>> GetStruct<T> for B {
    fn get_struct(&mut self) -> T {
        T::from(self.clone())
    }
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