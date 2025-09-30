# CKB Vest

A vesting contract for the CKB blockchain that allows secure token distribution with time-based release schedules.

## What is it?

CKB Vest lets you create vesting schedules where tokens are locked and released gradually over time. This is commonly used for:

- Team token allocations
- Investor distributions
- Community rewards
- Any situation where you want to release tokens gradually instead of all at once

## How it works

1. **Creator** sets up a vesting schedule with:
   - Total amount of tokens
   - Start time (epoch)
   - End time (epoch)
   - Cliff period (tokens don't vest until this point)

2. **Beneficiary** can claim vested tokens as they become available over time

3. **Creator** can terminate early and reclaim unvested tokens (all-or-nothing)

## Example

If you vest 1000 tokens from epoch 100 to epoch 200 with a cliff at epoch 120:
- Before epoch 120: No tokens can be claimed
- At epoch 150: 50% of tokens (500) are vested and claimable
- At epoch 200: All tokens are vested

## Development

```bash
cd contracts
make              # Build the contract
cargo test        # Run tests
```

The contract is implemented as a CKB lock script in Rust. A frontend interface is planned for future development.

## License

MIT
