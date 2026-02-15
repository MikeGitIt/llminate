#[cfg(test)]
mod basic_simple_tests {
    #[test]
    fn test_basic_arithmetic() {
        // Test basic arithmetic operations
        assert_eq!(5 + 3, 8, "Addition failed");
        assert_eq!(10 - 4, 6, "Subtraction failed");
        assert_eq!(3 * 4, 12, "Multiplication failed");
        assert_eq!(15 / 3, 5, "Division failed");
    }

    #[test]
    fn test_string_operations() {
        // Test string concatenation
        let s1 = "Hello, ";
        let s2 = "World!";
        let combined = format!("{}{}", s1, s2);
        
        assert_eq!(combined, "Hello, World!", "String concatenation failed");
        
        // Test string length
        assert_eq!(combined.len(), 13, "String length calculation failed");
    }

    #[test]
    fn test_boolean_logic() {
        // Test boolean operations
        let a = true;
        let b = false;
        
        assert_eq!(a && b, false, "AND operation failed");
        assert_eq!(a || b, true, "OR operation failed");
        assert_eq!(!a, false, "NOT operation failed");
        assert_eq!(!b, true, "NOT operation failed");
    }
}