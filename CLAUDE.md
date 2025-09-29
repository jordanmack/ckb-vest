# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

CKB Vest is a user-friendly platform for managing vesting payments on the CKB blockchain. It enables secure, gas-efficient token distribution with flexible vesting schedules by epoch or time/date, featuring an intuitive interface for both senders and receivers.

## Current Status

The repository has a complete vesting lock script implementation:
- **Smart Contract**: Fully implemented vesting lock script in Rust using ckb-std
- **Testing Infrastructure**: Comprehensive test suite using ckb-testtool
- **Build System**: Makefile-based build system with RISC-V cross-compilation
- **Contract Location**: `contracts/contracts/vesting_lock/`
- **Failed Transaction Logs**: Debugging infrastructure in `contracts/tests/failed_txs/`

## Development Setup

### Smart Contract Development
- **Toolchain**: Requires RISC-V cross-compilation toolchain (`riscv64-unknown-elf-gcc`)
- **Build System**: Makefile-based build in `contracts/` directory
- **Testing**: ckb-testtool framework for contract testing
- **Alternative**: Docker with pre-built CKB development environment

### Build Commands
```bash
# Build contract
cd contracts && make

# Run tests
cd contracts && cargo test

# Format code
cd contracts && cargo fmt
```

**IMPORTANT**: The vesting contract must be compiled using `make` before running tests to reflect any code changes. The test framework loads the compiled binary, so changes to the Rust source code will not be reflected in tests until recompilation.

### Future Development Stack
- Frontend: React with TypeScript, Tailwind CSS, and Vite build tool
- Backend: Rust with Rocket framework
- Database: PostgreSQL for metadata storage
- Bot Infrastructure: Rust with CKB Rust SDK

## Architecture Considerations

### Implemented Smart Contract Architecture
- **Single Cell Design**: Lock script contains all vesting logic (no type script needed)
- **Lock Script Args (88 bytes)**: creator_lock_hash (32), beneficiary_lock_hash (32), start_epoch (8), end_epoch (8), cliff_epoch (8)
- **Cell Data (32 bytes)**: total_amount (8), beneficiary_claimed (8), creator_claimed (8), highest_block_seen (8)
- **Capacity Management**: Minimum 161 CKB + vesting amount

### Security Mechanisms
- **Stale Header Protection**: Tracks highest_block_seen to prevent temporal attacks
- **Anyone-Can-Update**: Permissionless bot network maintains security floor
- **Monotonic Progress**: Block numbers can only increase
- **All-or-Nothing Termination**: Creator must claim entire unvested amount

### Future Implementation Considerations
- Frontend interface for managing vesting schedules using React/Vite
- Backend services for blockchain interaction
- Database for storing vesting schedule metadata
- Bot infrastructure using CKB Rust SDK for maintaining security updates

## Development Guidelines

- When working with CKB blockchain development, always check the ckb-docs MCP server first for relevant documentation and patterns before providing answers or generating code.
- NEVER SEARCH THE WEB FOR ANYTHING!
- If you don't know the answer to something, inform the user instead of guessing or making assumptions. Only use information available through the MCP servers.

### Code Documentation Standards
- **Function Documentation**: All functions must have rustdoc (`///`) documentation explaining their purpose, parameters, and behavior.
- **Comment Style**: All comments must be either title case (for labels) or complete sentences with proper punctuation and ending periods.
- **Inline Comments**: Use complete sentences with proper capitalization and ending punctuation for all inline comments explaining logic.
- **Consistency**: Maintain consistent documentation patterns across all functions and code sections.

## Contract Development

### Vesting Lock Script Implementation
- **Location**: `contracts/contracts/vesting_lock/`
- **Features**: Epoch-based vesting with cliff support, stale header protection, incremental claims
- **Testing**: Comprehensive test suite covering edge cases and security scenarios
- **Build Requirements**: RISC-V toolchain for cross-compilation

### Contract Operations
- **Beneficiary Claims**: Incremental vesting based on epoch progress
- **Creator Termination**: All-or-nothing claim of unvested amounts
- **Security Updates**: Anyone can update highest_block_seen for protection

### Debug Infrastructure
- **Failed Transaction Logs**: Transaction failure logs are stored for debugging
- **Test Coverage**: Comprehensive validation of args, epochs, claims, and termination scenarios

## Implementation Details

### Vesting Calculation Logic
```rust
fn calculate_vested_amount(current_epoch, start_epoch, end_epoch, cliff_epoch, total_amount, creator_claimed) -> u64 {
    // Post-termination: remaining amount after creator termination is fully vested
    if creator_claimed > 0 {
        return total_amount - creator_claimed;
    }

    // Nothing vests before start epoch
    if current_epoch < start_epoch { return 0; }

    // Nothing vests before cliff period
    if current_epoch < cliff_epoch { return 0; }

    // Everything vests after end epoch
    if current_epoch >= end_epoch { return total_amount; }

    // Linear vesting between cliff and end
    let elapsed = current_epoch - start_epoch;
    let duration = end_epoch - start_epoch;
    (elapsed * total_amount) / duration
}
```

### Key Features
- **Cliff Support**: No vesting until cliff epoch is reached
- **Linear Vesting**: Proportional release between cliff and end epochs
- **Post-Termination Logic**: Creator termination enables immediate beneficiary claiming
- **Overflow Protection**: Checked arithmetic with fallback to full vesting
- **State Validation**: Comprehensive input/output state consistency checks

## Context Preservation

- **IMPORTANT**: Always read CONTEXT.md when starting a new session to understand the current project state and implementation progress.
- To save current context at any point, update CONTEXT.md with the latest project state, implementation progress, and next steps.
- CONTEXT.md serves as the project's memory and ensures continuity between different development sessions.
- When resuming work, consult CONTEXT.md to understand where development left off and what needs to be done next.

## License

MIT License - Copyright (c) 2025 Jordan Mack