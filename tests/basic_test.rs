#[cfg(test)]
mod basic_tests {
    #[test]
    fn test_multiplication() {
        // A simple test that verifies basic multiplication
        let result = 3 * 4;
        assert_eq!(result, 12, "3 * 4 should equal 12");
    }

    #[test]
    fn test_string_concatenation() {
        // A simple test that verifies string concatenation
        let s1 = "hello";
        let s2 = " world";
        let combined = format!("{}{}", s1, s2);
        assert_eq!(combined, "hello world", "String concatenation should work correctly");
    }

    #[test]
    fn test_vector_operations() {
        // A simple test that verifies vector operations
        let mut vec = vec![1, 2, 3];
        vec.push(4);
        assert_eq!(vec.len(), 4, "Vector should have 4 elements");
        assert_eq!(vec[3], 4, "The fourth element should be 4");
    }
}