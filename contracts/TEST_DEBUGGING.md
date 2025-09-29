# CKB Contract Testing and Debugging Guide

This guide documents the enhanced error reporting and debugging approaches for CKB smart contract testing, following official CKB ecosystem best practices.

## Enhanced Error Code Display

### Current Implementation

The project now includes enhanced error code extraction and display:

```rust
// Enhanced error code extraction with multiple patterns
fn extract_error_code(result: &Result<Cycle, Error>) -> Option<i8> {
    // Pattern 1: "see error code XX" (standard CKB pattern)
    // Pattern 2: Direct ValidationFailure error code  
    // Pattern 3: Error code followed by "on page" pattern
}
```

### Test Output Format

All test assertions now include error codes in failure messages:

```
Should succeed - zero vesting amount is valid for security updates, got error code: Some(13)
Should fail - claiming before start epoch, got error code: Some(21)
```

## CLI Options for Enhanced Debugging

### Standard Cargo Test Options

```bash
# Show all output (including println! statements)
cargo test -- --nocapture

# Run tests sequentially (prevents interleaved output)
cargo test -- --test-threads 1

# Continue running all tests even after failures  
cargo test --no-fail-fast -- --nocapture

# Run with verbose output
cargo test -v -- --nocapture

# Run specific test with full output
cargo test test_zero_vesting_amount -- --nocapture --exact
```

### Environment Variables for Enhanced Debugging

```bash
# Enable detailed error logging
export RUST_LOG=debug
cargo test -- --nocapture

# Run tests sequentially for ordered output
export RUST_TEST_THREADS=1
cargo test -- --nocapture

# Enable overflow checks in release mode (for debugging)
export RUSTFLAGS="--cfg debug_assertions"
cargo test --release -- --nocapture

# Project-specific debug mode (when implemented)
export CKB_TEST_DEBUG=true
cargo test -- --nocapture
```

### Combined Debugging Command

For maximum debugging information:

```bash
RUST_LOG=debug RUST_TEST_THREADS=1 CKB_TEST_DEBUG=true cargo test test_zero_vesting_amount -- --nocapture --exact
```

## Error Code Reference

### Contract-Specific Error Codes

Based on `contracts/contracts/vesting_lock/src/error.rs`:

- **1**: IndexOutOfBound - Trying to access non-existent cell/witness
- **2**: ItemMissing - Required data is missing
- **3**: LengthNotEnough - Buffer too small for data
- **4**: InvalidData - General data validation failure
- **10**: InvalidArgs - Script arguments invalid
- **11**: InvalidWitness - Witness structure invalid
- **12**: InvalidTransaction - Transaction structure invalid
- **13**: InvalidTransactionStructure - Wrong input/output count
- **14**: TotalAmountChanged - Vesting total amount modified
- **20**: InvalidAmount - Amount validation failed
- **21**: InsufficientVested - Claiming more than vested
- **22**: AlreadyTerminated - Vesting already terminated
- **23**: InvalidEpoch - Epoch ordering invalid
- **24**: StaleHeader - Header dependency outdated
- **30**: InvalidCellData - Cell data structure invalid

### Common Error Patterns

| Error Code | Common Cause | Solution |
|------------|--------------|----------|
| 13 | Transaction has wrong number of inputs/outputs | Check transaction structure in test |
| 10 | Script args wrong length or format | Validate 88-byte args structure |
| 23 | Invalid epoch ordering (start >= end) | Fix epoch parameters in test |
| 21 | Beneficiary claiming too much | Check vesting calculation |
| 24 | Highest block seen decreased | Ensure monotonic block updates |

## Debugging Workflow

### Step 1: Basic Error Identification

```bash
# Run failing test to see error code
cargo test test_name -- --nocapture
```

Look for output like: `got error code: Some(13)`

### Step 2: Enhanced Debugging

```bash
# Enable full debugging output
RUST_LOG=debug RUST_TEST_THREADS=1 cargo test test_name -- --nocapture --exact
```

### Step 3: Transaction Structure Analysis

For error code 13 (InvalidTransactionStructure), check:

1. **Input Count**: Does the test create exactly one group input?
2. **Output Count**: Does the test create exactly one group output?
3. **Cell Dependencies**: Are all required cell deps included?
4. **Witness Structure**: Is the witness properly formatted?

### Step 4: Contract-Specific Debugging

Add debug output to contract (for development):

```rust
// In contract code (development only)
ckb_std::debug!("Input count: {}", input_count);
ckb_std::debug!("Output count: {}", output_count);
```

Then run with CKB debugger:

```bash
# Using ckb-standalone-debugger (if available)
ckb-debugger --bin contract.bin --tx transaction.json
```

## Best Practices from CKB Ecosystem

### 1. Error Message Format

Following CKB standards, our error messages include:
- Test name for context
- Expected vs actual behavior  
- Numeric error code for quick identification
- Full error details when debug mode is enabled

### 2. Environment Variable Support

- `RUST_LOG=debug`: Standard Rust logging
- `RUST_TEST_THREADS=1`: Sequential test execution
- `CKB_TEST_DEBUG=true`: Project-specific enhanced debugging

### 3. CLI Options

- `--nocapture`: Essential for seeing debug output
- `--test-threads 1`: Prevents interleaved output
- `--no-fail-fast`: See all test failures at once
- `--exact`: Run specific test without pattern matching

### 4. Error Code Standards

- Use distinct exit codes (-128 to 127) for different errors
- Document error codes in community error codes repository
- Provide clear, actionable error messages

## Integration with Development Tools

### VS Code Configuration

Add to `.vscode/launch.json`:

```json
{
    "configurations": [
        {
            "name": "Debug CKB Tests",
            "type": "lldb",
            "request": "launch",
            "program": "${workspaceFolder}/target/debug/deps/tests-*",
            "args": ["test_name", "--nocapture", "--exact"],
            "env": {
                "RUST_LOG": "debug",
                "RUST_TEST_THREADS": "1",
                "CKB_TEST_DEBUG": "true"
            }
        }
    ]
}
```

### Makefile Integration

Add to `Makefile`:

```makefile
# Debug specific test
debug-test:
	RUST_LOG=debug RUST_TEST_THREADS=1 CKB_TEST_DEBUG=true \
	cargo test $(TEST) -- --nocapture --exact

# Run all tests with error codes
test-with-errors:
	cargo test -- --nocapture --test-threads 1
```

Usage:
```bash
make debug-test TEST=test_zero_vesting_amount
make test-with-errors
```

## Comparison with Other CKB Projects

Our approach aligns with patterns found in:

- **ckb-zkp**: Uses `--nocapture` and environment variables
- **Official CKB repositories**: Error code documentation and CLI debugging
- **Community projects**: Structured error reporting and debug modes

## Migration from Previous Approaches

If updating from basic error handling:

1. **Replace simple assertions**:
   ```rust
   // Old
   assert!(result.is_ok());
   
   // New  
   assert!(result.is_ok(), "Test failed with error code: {:?}", extract_error_code(&result));
   ```

2. **Add error code constants**:
   ```rust
   const ERROR_INVALID_ARGS: i8 = 10;
   const ERROR_INVALID_EPOCH: i8 = 23;
   ```

3. **Use enhanced CLI options**:
   ```bash
   # Replace
   cargo test
   
   # With
   cargo test -- --nocapture --test-threads 1
   ```

This enhanced approach provides the debugging capabilities expected in professional CKB development environments while maintaining compatibility with existing test infrastructure.