#[cfg(test)]
mod tests {
    // Basic function to test
    fn add(a: i32, b: i32) -> i32 {
        a + b
    }
    
    // Basic function to test
    fn subtract(a: i32, b: i32) -> i32 {
        a - b
    }
    
    // Basic function to test
    fn multiply(a: i32, b: i32) -> i32 {
        a * b
    }
    
    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
        assert_eq!(add(0, 0), 0);
        assert_eq!(add(-1, 1), 0);
    }
    
    #[test]
    fn test_subtract() {
        assert_eq!(subtract(5, 3), 2);
        assert_eq!(subtract(0, 0), 0);
        assert_eq!(subtract(1, 1), 0);
    }
    
    #[test]
    fn test_multiply() {
        assert_eq!(multiply(2, 3), 6);
        assert_eq!(multiply(0, 5), 0);
        assert_eq!(multiply(-2, 3), -6);
    }
}