# Vesting Lock Contract

A CKB smart contract that implements epoch-based token vesting with security features.

## Overview

The vesting lock contract enables secure, time-locked token distribution on the CKB blockchain. It supports:

- **Epoch-based vesting**: Tokens vest linearly over a specified epoch range
- **Cliff period**: Optional delay before vesting begins
- **Creator termination**: Contract creator can reclaim unvested tokens
- **Stale header protection**: Prevents attacks using outdated blockchain state
- **Anyone-can-update**: Community can maintain contract security

## Contract Specification

### Lock Script Args (88 bytes)
- `creator_lock_hash` (32 bytes): Hash of creator's lock script
- `beneficiary_lock_hash` (32 bytes): Hash of beneficiary's lock script  
- `start_epoch` (8 bytes): Epoch when vesting begins
- `end_epoch` (8 bytes): Epoch when vesting completes
- `cliff_epoch` (8 bytes): Epoch when cliff period ends

### Cell Data (32 bytes)
- `total_amount` (8 bytes): Total tokens to vest
- `beneficiary_claimed` (8 bytes): Tokens claimed by beneficiary
- `creator_claimed` (8 bytes): Tokens claimed by creator
- `highest_block_seen` (8 bytes): Highest block number processed

## Security Features

1. **Stale Header Protection**: Contract tracks the highest block number seen and rejects transactions that reference older blocks, preventing attackers from using stale blockchain state.

2. **Anyone-Can-Update**: Any user can update the `highest_block_seen` field to maintain security without requiring the creator or beneficiary to act.

3. **All-or-Nothing Termination**: When creators terminate vesting, they must claim all remaining unvested tokens in a single transaction.

## Building

```bash
make build
```

## Testing

```bash
make test
```

## Error Codes

- `10`: Invalid arguments
- `12`: Invalid transaction structure
- `20`: Invalid amount
- `21`: Insufficient vested tokens
- `22`: Already terminated
- `23`: Invalid epoch ordering
- `24`: Stale header detected
- `25`: Unauthorized operation
- `30`: Invalid cell data

*This contract was bootstrapped with [ckb-script-templates].*

[ckb-script-templates]: https://github.com/cryptape/ckb-script-templates
