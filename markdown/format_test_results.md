# Output Formatting Test Results

This file demonstrates various types of output formatting.

## Code Block Examples

### Rust Code
```rust
fn main() {
    println!("Hello, world!");
    
    // A simple calculation
    let x = 5;
    let y = 10;
    let result = x + y;
    
    println!("The result is: {}", result);
}
```

### Python Code
```python
def greet(name):
    """A simple greeting function"""
    return f"Hello, {name}!"

# Example usage
if __name__ == "__main__":
    print(greet("User"))
    
    # A simple calculation
    x = 5
    y = 10
    result = x + y
    
    print(f"The result is: {result}")
```

## Table Example

| Name | Type | Description |
|------|------|-------------|
| id | integer | Unique identifier |
| name | string | User's full name |
| email | string | User's email address |
| created_at | timestamp | Account creation time |

## List Examples

### Ordered List
1. First item
2. Second item
3. Third item
   1. Sub-item 3.1
   2. Sub-item 3.2
4. Fourth item

### Unordered List
- Main point
  - Supporting detail
  - Another detail
- Second point
- Third point

## Text Formatting

**Bold text** is useful for emphasis.
*Italic text* can also be used for emphasis.
~~Strikethrough~~ shows deleted content.

> This is a blockquote that can be used to highlight important information or quotes.

---

## Terminal Output Example

```
$ ls -la
total 128
drwxr-xr-x  15 user  staff   480 Oct 15 14:30 .
drwxr-xr-x   5 user  staff   160 Oct 10 09:15 ..
-rw-r--r--   1 user  staff  2458 Oct 15 14:30 README.md
-rw-r--r--   1 user  staff  1797 Oct 14 16:42 Cargo.toml
drwxr-xr-x   8 user  staff   256 Oct 15 13:22 src
drwxr-xr-x  12 user  staff   384 Oct 15 13:45 target
drwxr-xr-x   4 user  staff   128 Oct 14 11:30 tests
```