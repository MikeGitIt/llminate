# Output Formatting Demonstration

This file demonstrates various output formatting capabilities.

## Text Formatting

**Bold text** is created with double asterisks.
*Italic text* is created with single asterisks.
`Inline code` is created with backticks.
~~Strikethrough~~ is created with double tildes.

## Code Blocks

Code blocks can be created with triple backticks and an optional language identifier:

```python
def hello_world():
    print("Hello, World!")
    return True
```

```javascript
function helloWorld() {
  console.log("Hello, World!");
  return true;
}
```

```rust
fn main() {
    println!("Hello, World!");
}
```

## Tables

Tables can be created with pipes and dashes:

| Name     | Type    | Description                 |
|----------|---------|----------------------------|
| id       | integer | Unique identifier          |
| name     | string  | User's full name           |
| email    | string  | User's email address       |
| active   | boolean | Whether user is active     |

## Lists

### Unordered Lists

- Item 1
- Item 2
  - Nested item 2.1
  - Nested item 2.2
- Item 3

### Ordered Lists

1. First step
2. Second step
   1. Substep 2.1
   2. Substep 2.2
3. Third step

## Blockquotes

> This is a blockquote.
> It can span multiple lines.
>
> And even multiple paragraphs.

## Horizontal Rule

---

## Links

[Example Link](https://example.com)

## Images

Images can be included with the following syntax:
![Alt text](https://example.com/image.jpg)

## JSON Example

```json
{
  "name": "John Doe",
  "age": 30,
  "isActive": true,
  "address": {
    "street": "123 Main St",
    "city": "Anytown",
    "zipCode": "12345"
  },
  "phoneNumbers": [
    "555-1234",
    "555-5678"
  ]
}
```

## Command Line Output

Command line output can be formatted as code blocks:

```bash
$ ls -la
total 32
drwxr-xr-x   5 user  group   160 Oct 10 15:30 .
drwxr-xr-x  14 user  group   448 Oct  9 12:45 ..
-rw-r--r--   1 user  group  1024 Oct 10 15:30 file1.txt
-rw-r--r--   1 user  group  2048 Oct 10 15:30 file2.txt
drwxr-xr-x   3 user  group    96 Oct 10 15:30 directory
```

## Tool Output Formatting

Tool output is displayed with proper formatting:

<tool_output>
{
  "result": "success",
  "data": [1, 2, 3, 4, 5],
  "metadata": {
    "timestamp": "2023-10-10T15:30:00Z",
    "source": "example_tool"
  }
}
</tool_output>

## Nested Elements

* Main item 1
  * Sub item 1.1
  * Sub item 1.2
* Main item 2
  * Sub item 2.1
    * Sub-sub item 2.1.1