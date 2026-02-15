#[cfg(test)]
mod basic_numeric_tests {
    #[test]
    fn test_multiplication() {
        // A simple test that verifies basic multiplication
        let result = 3 * 4;
        assert_eq!(result, 12, "3 * 4 should equal 12");
    }

    #[test]
    fn test_division() {
        // A simple test that verifies basic division
        let result = 10 / 2;
        assert_eq!(result, 5, "10 / 2 should equal 5");
    }

    #[test]
    fn test_remainder() {
        // A simple test that verifies modulo operation
        let result = 10 % 3;
        assert_eq!(result, 1, "10 % 3 should equal 1");
    }

    #[test]
    fn test_greater_than() {
        // A simple test that verifies comparison
        let value = 5;
        assert!(value > 3, "5 should be greater than 3");
    }
}