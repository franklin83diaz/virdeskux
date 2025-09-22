pub fn capture() -> Vec<u8> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_test() {
        let result = capture();
        assert_eq!(result.len(), 0);
    }
}
