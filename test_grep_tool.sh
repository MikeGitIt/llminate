#!/bin/bash

# Test script for Grep tool in the TUI application

echo "Testing Grep tool in the TUI application..."

# Create test files for grep search
mkdir -p test_grep_files
echo "This is a test file with some content" > test_grep_files/file1.txt
echo "Another file with test content" > test_grep_files/file2.txt
echo "function testFunction() { return 42; }" > test_grep_files/code.js
echo "pub fn rust_function() -> i32 { 42 }" > test_grep_files/code.rs

# Test 1: Basic pattern search
echo "Test 1: Basic pattern search for 'test'"
echo "search for 'test' in the test_grep_files directory" | cargo run

# Give it time to process
sleep 3

# Test 2: Regex pattern search
echo -e "\nTest 2: Regex pattern search for function definitions"
echo "find all function definitions using regex pattern 'function\\s+\\w+' in test_grep_files" | cargo run

sleep 3

# Test 3: Case insensitive search
echo -e "\nTest 3: Case insensitive search"
echo "search for 'TEST' case insensitively in test_grep_files" | cargo run

sleep 3

# Test 4: File type filtering
echo -e "\nTest 4: Search only in JavaScript files"
echo "search for 'function' in JavaScript files in test_grep_files" | cargo run

sleep 3

# Cleanup
rm -rf test_grep_files

echo "Grep tool testing completed!"