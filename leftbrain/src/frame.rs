//! Cheap class probes used before the brain.

pub fn solid_run(data: &[u8]) -> Option<u8> {
    if data.len() < 4096 {
        return None;
    }
    let b = data[0];
    if data.iter().all(|&x| x == b) {
        Some(b)
    } else {
        None
    }
}

pub fn sparse_mode(data: &[u8]) -> Option<()> {
    if data.len() < 256 {
        return None;
    }
    let n = data.len().min(64 * 1024);
    let zeros = data[..n].iter().filter(|&&b| b == 0).count();
    if zeros * 100 >= n * 98 {
        Some(())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_4k() {
        let v = vec![0u8; 4096];
        assert_eq!(solid_run(&v), Some(0));
        assert!(solid_run(&[1u8; 100]).is_none());
    }
}
