#[cfg(test)]
mod simple_tests {
    #[test]
    fn test_addition() {
        // A simple test that verifies basic addition
        let result = 2 + 2;
        assert_eq!(result, 4, "2 + 2 should equal 4");
    }

    #[test]
    fn test_string_equality() {
        // A simple test that verifies string equality
        let s1 = "hello";
        let s2 = "hello";
        assert_eq!(s1, s2, "Strings should be equal");
    }

    #[test]
    fn test_boolean_assertion() {
        // A simple test that verifies a boolean condition
        let value = true;
        assert!(value, "Value should be true");
    }
}