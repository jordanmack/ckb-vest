# CKB Vest Contracts

Smart contracts for the CKB Vest platform - a user-friendly vesting payment system on the CKB blockchain.

## Overview

This repository contains the smart contracts that power CKB Vest, enabling secure, gas-efficient token distribution with flexible vesting schedules. The platform supports both epoch-based and time-based vesting with an intuitive interface for both senders and receivers.

## Contracts

### Vesting Lock (`contracts/vesting_lock/`)

The core vesting contract that implements:
- Epoch-based linear vesting with cliff support
- Creator termination capabilities
- Stale header attack protection
- Anyone-can-update security maintenance
- Comprehensive error handling

**Key Features:**
- **Security First**: Built-in protection against common attack vectors
- **Gas Efficient**: Optimized for minimal transaction costs
- **Flexible**: Supports various vesting schedules and cliff periods
- **Community Maintained**: Anyone can help maintain contract security

## Quick Start

### Prerequisites
- Rust (latest stable)
- RISC-V toolchain
- CKB development environment

### Building All Contracts

```bash
make build
```

### Running Tests

```bash
make test
```

### Development Tools

```bash
make check    # Run cargo check
make clippy   # Run linting
make fmt      # Format code
```

## Architecture

The vesting system uses a single-cell design pattern where each vesting arrangement is represented by one CKB cell protected by the vesting lock script. This approach provides:

- **Simplicity**: Easy to understand and audit
- **Efficiency**: Minimal on-chain storage and computation
- **Security**: Clear ownership and access control

## Development

### Adding New Contracts

Generate a new contract from template:

```bash
make generate CRATE=new_contract_name
```

### Testing Strategy

- **Unit Tests**: Core logic validation
- **Integration Tests**: Full transaction simulation
- **Security Tests**: Attack vector validation

## Security Considerations

- All contracts implement protection against common attack patterns
- Comprehensive input validation and error handling
- Regular security audits recommended for production use
- Anyone-can-update patterns for community maintenance

## License

MIT License - See individual contract directories for specific licensing information.

*This project was bootstrapped with [ckb-script-templates].*

[ckb-script-templates]: https://github.com/cryptape/ckb-script-templates
