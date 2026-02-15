#[cfg(test)]
mod new_tests {
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
    fn test_string_concatenation() {
        // A simple test that verifies string concatenation
        let s1 = "Hello, ";
        let s2 = "world!";
        let combined = format!("{}{}", s1, s2);
        assert_eq!(combined, "Hello, world!", "String concatenation failed");
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