//! IEEE CRC-32. Shared by PCC1 v2 and .pcc archives.

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    #[test]
    fn ieee_known() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}
