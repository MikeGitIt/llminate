#[cfg(test)]
mod tests {
    #[test]
    fn test_addition() {
        // A simple test that verifies basic addition
        let result = 2 + 2;
        assert_eq!(result, 4);
    }

    #[test]
    fn test_subtraction() {
        // A simple test that verifies basic subtraction
        let result = 5 - 3;
        assert_eq!(result, 2);
    }

    #[test]
    fn test_multiplication() {
        // A simple test that verifies basic multiplication
        let result = 3 * 4;
        assert_eq!(result, 12);
    }
}