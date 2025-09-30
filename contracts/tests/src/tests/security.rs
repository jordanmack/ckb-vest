use super::helpers::*;
use crate::Loader;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

/// Tests protection against stale header attacks where highest_block_seen decreases.
/// Ensures the contract rejects attempts to roll back the highest observed block number.
#[test]
fn test_stale_header_attack_prevention() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let beneficiary_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 250), // highest_block_seen = 250
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Trying to use stale header (decreasing highest_block_seen)
    let output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 5000, 0, 200).pack()) // trying to decrease to 200
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - stale header attack (decreasing highest_block_seen), got error code: {:?}", extract_error_code(&result));
}

/// Tests that highest_block_seen can increase monotonically during valid operations.
/// Validates that the security mechanism allows forward progress.
#[test]
fn test_highest_block_seen_monotonic_increase() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201, higher than input's highest_block_seen (200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // highest_block_seen = 200
    );

    // Create beneficiary authorization input cell
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Valid partial claim with continuing vesting cell
    let beneficiary_output = CellOutput::new_builder()
        .capacity(5000u64.pack()) // claiming 50% vested amount
        .lock(beneficiary_lock)
        .build();

    // Continuing vesting cell with remaining amount
    let vesting_output = CellOutput::new_builder()
        .capacity(5000u64.pack()) // remaining capacity
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(beneficiary_output)
        .output_data(Bytes::new().pack())
        .output(vesting_output)
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack()) // claimed 5000, updated block
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - monotonic increase of highest_block_seen, got error code: {:?}", extract_error_code(&result));
}

/// Tests that highest_block_seen cannot decrease in anyone-can-update operations.
/// Ensures the monotonic property is enforced even for unauthorized updates.
#[test]
fn test_highest_block_seen_cannot_decrease() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 300),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Anyone-can-update: just updating highest_block_seen (no claims)
    let output = CellOutput::new_builder()
        .capacity(10000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 250).pack()) // trying to decrease
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - highest_block_seen cannot decrease, got error code: {:?}", extract_error_code(&result));
}

/// Tests the anyone-can-update functionality for maintaining security.
/// Validates that any user can update highest_block_seen without special authorization.
#[test]
fn test_anyone_can_update_highest_block() {
    // Test: Anyone can update highest_block_seen without authorization
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with new block number and epoch for update
    let header_hash = setup_header_with_block_and_epoch(&mut context, 350, 350);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 vesting + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // old highest_block_seen = 200
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Anyone-can-update: just updating highest_block_seen (no authorization needed)
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 350).pack()) // updated highest_block_seen = 350
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - anyone can update highest_block_seen, got error code: {:?}", extract_error_code(&result));
}

#[test]
fn test_stale_header_rejection() {
    // Test: Contract rejects transactions with stale headers
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);
    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );
    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup stale header (block 150 < input's highest_block_seen 200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 150, 150);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 vesting + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // highest_block_seen = 200
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Try to update with stale header
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 150).pack()) // trying to use stale block
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - stale header rejection");
    assert_eq!(extract_error_code(&result), Some(24)); // Error::StaleHeader (header freshness check)
}

#[test]
fn test_mismatched_output_block_number() {
    // Test: Contract enforces that output highest_block_seen matches highest header
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);
    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );
    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup fresh header (block 350 > input's highest_block_seen 200)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 350, 350);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 vesting + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // highest_block_seen = 200
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Try to set output highest_block_seen higher than any header provides
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 400).pack()) // 400 > 350 from header!
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - mismatched output block number");
    assert_eq!(extract_error_code(&result), Some(27)); // Error::BlockNumberMismatch
}

/// Tests anonymous update when cell is fully distributed but not consumed.
/// Validates that security updates work when total_amount equals beneficiary_claimed + creator_claimed.
#[test]
fn test_anonymous_update_fully_distributed_cell() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with new block number
    let header_hash = setup_header_with_block_and_epoch(&mut context, 351, 350);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(2161u64.pack()) // Minimal capacity + small amount
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(2000, 1200, 800, 300), // total = beneficiary_claimed + creator_claimed
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Anonymous update: just updating highest_block_seen
    let output = CellOutput::new_builder()
        .capacity(2161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(2000, 1200, 800, 351).pack()) // only block update
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - anonymous update when fully distributed, got error code: {:?}", extract_error_code(&result));
}

/// Tests anonymous update after creator termination.
/// Validates that security updates work in post-termination state.
#[test]
fn test_anonymous_update_post_termination() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with new block number
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 4000, 200), // Post-termination state: creator claimed 4000
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Anonymous update: just updating highest_block_seen in post-termination state
    let output = CellOutput::new_builder()
        .capacity(6161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 2000, 4000, 251).pack()) // only block update
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - anonymous update post-termination, got error code: {:?}", extract_error_code(&result));
}

/// Tests anonymous update when cell is fully vested but unclaimed.
/// Validates that security updates work when tokens are fully vested but nobody has claimed them yet.
#[test]
fn test_anonymous_update_fully_vested_unclaimed() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        200, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with epoch past end_epoch
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // Fully vested (epoch 250 > end_epoch 200), but nothing claimed
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Anonymous update: just updating highest_block_seen on fully vested but unclaimed cell
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(create_vesting_data(10000, 0, 0, 251).pack()) // only block update
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - anonymous update on fully vested unclaimed cell, got error code: {:?}", extract_error_code(&result));
}

/// Tests validation of invalid creator claim delta in termination operations.
/// Ensures the contract rejects incorrect creator claimed amounts.
#[test]
fn test_invalid_creator_delta() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (_beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 200 (50% vested)
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested = 5000
    );

    // Create creator authorization input cell
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Creator terminating but with wrong claimed amount in state
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 0, 4500, 201).pack()) // Wrong: should be 5000, not 4500
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - invalid creator claimed delta");

    // Should fail with InvalidAmount since the all-or-nothing check happens first
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 20, "Expected error code 20 (InvalidAmount), got {}", error_code);
    }
}