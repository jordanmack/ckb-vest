# CKB Vest Development Context

This file tracks the current state of the CKB Vest project and serves as context for development sessions.

## Current Project State

### Completed Components
- ✅ **Vesting Lock Script**: Fully implemented and functional
- ✅ **Security Architecture**: Stale header protection, anyone-can-update mechanism
- ✅ **Vesting Logic**: Linear vesting with cliff support and post-termination handling
- ✅ **Test Suite**: Comprehensive test coverage with 40+ test cases
- ✅ **Build System**: RISC-V cross-compilation setup with Makefile

### Implementation Status
- **Smart Contract**: Complete and production-ready
- **Contract Features**:
  - Epoch-based vesting with cliff periods
  - Creator termination with all-or-nothing policy
  - Beneficiary incremental claiming
  - Post-termination beneficiary claiming
  - Stale header attack prevention
  - Monotonic block number progression
- **Testing**: All edge cases covered including batched operations, security scenarios

### Architecture Details
- **Contract Type**: Lock script (no type script required)
- **Data Structures**: 88-byte args, 32-byte cell data
- **Capacity Requirements**: Minimum 161 CKB + vesting amount
- **Security Model**: Proxy lock pattern for authorization

## Next Development Phases

### Phase 1: Frontend Development (Planned)
- React application with TypeScript and Tailwind CSS
- Vite build tool for development and bundling
- CCC SDK integration for CKB blockchain interaction
- User interface for creating and managing vesting schedules
- Wallet integration for transaction signing
- Setup using CLI tools following standard human development workflow

### Phase 2: Backend Services (Planned)
- Rust backend with Rocket framework
- PostgreSQL database for metadata storage
- API endpoints for vesting schedule management
- Transaction building and broadcasting services

### Phase 3: Bot Infrastructure (Planned)
- Rust-based bot network using CKB Rust SDK
- Anyone-can-update security maintenance bots
- Automated block number update services
- Monitoring and alerting systems
- Setup using standard Rust development practices

## Development Notes

### Recent Updates
- Documentation cleaned up to remove volatile implementation details
- CLAUDE.md updated to focus on stable architectural patterns
- Vesting calculation logic updated to include post-termination scenarios
- Technology stack updated: React/Vite/Tailwind for frontend, CKB Rust SDK for bots
- Development approach defined: use CLI tools following standard human workflow

### Build Process
1. Smart contracts must be compiled with `make` before testing
2. Tests use compiled binaries, not source code directly
3. All changes require recompilation to be reflected in tests

### Testing Strategy
- Unit tests for individual functions
- Integration tests for complete transaction flows
- Edge case testing for security scenarios
- Batched operation rejection validation

## Known Issues
- None currently identified

## Deployment Status
- **Smart Contract**: Ready for mainnet deployment
- **Frontend**: Not started
- **Backend**: Not started
- **Bot Network**: Not started

Last Updated: September 29, 2025