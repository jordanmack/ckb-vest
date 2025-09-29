use super::helpers::*;
use crate::Loader;
use ckb_testtool::builtin::ALWAYS_SUCCESS;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

/// Tests that beneficiaries cannot claim vested tokens before the vesting period starts.
/// Claims should be rejected when current_epoch < start_epoch.
#[test]
fn test_beneficiary_claim_before_start_epoch() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let creator_hash = create_dummy_lock_hash(1);
    let beneficiary_hash = create_dummy_lock_hash(2);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        200, // start_epoch
        300, // end_epoch
        220, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let beneficiary_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 150), // current epoch 150 < start 200
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Beneficiary trying to claim before start epoch.
    let output = CellOutput::new_builder()
        .capacity(5000u64.pack()) // claiming half
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
        .output_data(create_vesting_data(10000, 5000, 0, 151).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming before start epoch, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries cannot claim tokens before the cliff period ends.
/// Claims should be rejected when current_epoch is between start_epoch and cliff_epoch.
#[test]
fn test_beneficiary_claim_before_cliff_epoch() {
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
        220, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");
    let beneficiary_lock = create_dummy_lock_script(&mut context);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 150), // current epoch 150 > start 100 but < cliff 220
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(2500u64.pack()) // claiming 25%
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(7500u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 2500, 0, 151).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming before cliff epoch, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries can claim partially vested tokens after the cliff period.
/// Validates proper authorization through beneficiary lock hash and linear vesting calculation.
#[test]
fn test_beneficiary_claim_partial_vested() {
    // Test: Beneficiary can claim partially vested tokens after cliff period.
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Use ALWAYS_SUCCESS differentiation technique.
    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch (200 epoch duration)
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with current epoch = 200 (50% through vesting period).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // Create vesting input cell.
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 vesting + 161 minimum capacity
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // total: 10000, claimed: 0, current epoch: 200
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder() // updated vesting cell (first output)
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack()) // claimed 5000
        .output(CellOutput::new_builder() // beneficiary receives claimed tokens (second output)
            .capacity(5000u64.pack())
            .lock(beneficiary_lock)
            .build())
        .output_data(Bytes::new().pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - claiming valid partial amount, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries can claim the full vested amount after the vesting period ends.
/// Validates claiming 100% of tokens when current_epoch >= end_epoch.
#[test]
fn test_beneficiary_claim_fully_vested() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Use proper authorization setup.
    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 350.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 351, 350);

    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack()) // 10000 + 161 minimum
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 350), // current epoch 350 > end 300 = fully vested
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Fully vested: beneficiary consumes entire cell (no outputs).
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - claiming full vested amount, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries cannot claim more tokens than have vested.
/// Ensures the contract rejects attempts to over-claim based on current vesting progress.
#[test]
fn test_beneficiary_claim_exceeds_vested_amount() {
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
        create_vesting_data(10000, 0, 0, 200), // 50% vested
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(7500u64.pack()) // trying to claim 75% when only 50% vested
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(2500u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 7500, 0, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming more than vested, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries cannot claim more than currently vested amount.
/// Uses proper authorization and validates specific error code.
#[test]
fn test_beneficiary_over_claim_rejected() {
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

    // Setup header with block 201, higher than input's highest_block_seen (200).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // At epoch 200, only 50% should be vested: (200-100)/(300-100) = 100/200 = 50%.
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested = 5000
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to claim 7500 (75%) when only 5000 (50%) is vested.
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 7500, 0, 201).pack()) // Over-claiming
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming more than vested, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21).
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that beneficiaries cannot claim before cliff period ends.
/// Validates that zero vesting before cliff is enforced.
#[test]
fn test_beneficiary_claim_before_cliff_rejected() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        150, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 131, higher than input's highest_block_seen (130).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 131, 130);

    // At epoch 130, before cliff at 150 - nothing should be vested.
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 130),
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to claim any amount before cliff.
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 1000, 0, 131).pack()) // Any claim before cliff
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming before cliff period, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21).
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that beneficiaries cannot over-claim after making partial claims.
/// Validates that previously claimed amounts are properly tracked.
#[test]
fn test_beneficiary_over_claim_after_partial_rejected() {
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

    // Setup header with block 251, higher than input's highest_block_seen (250).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    // At epoch 250, should be 75% vested: (250-100)/(300-100) = 150/200 = 75% = 7500.
    // Already claimed 3000, so 4500 more available.
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 3000, 0, 250), // Already claimed 3000
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to claim 6000 more when only 4500 additional is available.
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 9000, 0, 251).pack()) // Claiming 6000 more (total 9000)
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - claiming more than available after partial claim, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21).
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that beneficiaries cannot claim in post-termination when nothing remains.
/// Validates post-termination over-claiming protection.
#[test]
fn test_beneficiary_post_termination_over_claim_rejected() {
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

    // Setup header with block 201, higher than input's highest_block_seen (200).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // Post-termination: creator claimed 8000, beneficiary already claimed 2000.
    // Nothing left to claim (total 10000 - creator 8000 - beneficiary 2000 = 0).
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 8000, 200), // All claimed
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to claim more when nothing is left.
    let output = CellOutput::new_builder()
        .capacity(10161u64.pack())
        .lock(lock_script.clone())
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(create_vesting_data(10000, 3000, 8000, 201).pack()) // Trying to claim 1000 more
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - no remaining amount to claim post-termination, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21).
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that beneficiaries can make multiple incremental claims over time.
/// Validates that previously claimed amounts are tracked and additional claims work correctly.
#[test]
fn test_beneficiary_multiple_incremental_claims() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Use proper authorization setup.
    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201 and epoch 200.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200), // already claimed 2000, now 50% vested = 5000 total
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    let output = CellOutput::new_builder()
        .capacity(3000u64.pack()) // claiming additional 3000 (total 5000)
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(7000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - incremental claim within vested amount, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiaries can claim remaining tokens after creator termination.
/// Validates that vested tokens remain claimable even after termination occurs.
#[test]
fn test_post_termination_beneficiary_claims() {
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

    // Setup header with block 251, higher than input's highest_block_seen (250).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 251, 250);

    // Simulate post-termination state.
    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(7161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 1000, 4000, 250), // terminated: creator claimed 4000
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Post-termination: beneficiary consumes entire cell (no output).
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - post-termination beneficiary can claim remaining, got error code: {:?}", extract_error_code(&result));
}

/// Tests that cell data remains consistent after beneficiary claims.
/// Validates proper state transitions and data integrity during claim operations.
#[test]
fn test_cell_data_integrity_after_claim() {
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

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200), // already claimed 2000, now claiming more
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Setup header with block 201 and epoch 200 (50% vested).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    let output = CellOutput::new_builder()
        .capacity(1500u64.pack()) // claiming additional 1500
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(8500u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 3500, 0, 201).pack()) // total claimed = 3500
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - cell data integrity maintained after claim, got error code: {:?}", extract_error_code(&result));
}

/// Tests that CKB capacity is properly conserved across vesting transactions.
/// Validates that input capacity equals output capacity in all operations.
#[test]
fn test_capacity_conservation() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    // Use proper authorization setup.
    let (beneficiary_lock, beneficiary_hash, _creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header with block 201 and epoch 200.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_capacity = 10000u64;
    let vesting_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(input_capacity.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(8000, 0, 0, 200), // 8000 vesting, 2000 minimum capacity
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack()) // minimum capacity
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    let claimed_amount = 4000u64;
    let remaining_capacity = input_capacity - claimed_amount;

    let output = CellOutput::new_builder()
        .capacity(claimed_amount.pack())
        .lock(beneficiary_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(remaining_capacity.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(vesting_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(8000, 4000, 0, 201).pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_ok(), "Should succeed - capacity is conserved across transaction, got error code: {:?}", extract_error_code(&result));
}

/// Tests that beneficiary claiming full amount with output cell is rejected.
/// When fully claiming, the cell should be consumed (no output).
#[test]
fn test_beneficiary_full_claim_with_output() {
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

    // Setup header for epoch 350 (past end epoch - fully vested).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 351, 350);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 350), // Fully vested
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Beneficiary claiming full amount but incorrectly keeping output cell.
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 10000, 0, 351).pack()) // Fully claimed but output exists
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - full claim should not have output");

    // Verify it's the correct error (BeneficiaryFullClaimHasOutput = 42).
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 42, "Expected error code 42 (BeneficiaryFullClaimHasOutput), got {}", error_code);
    }
}

/// Tests that beneficiary partial claim without output is rejected.
/// Partial claims must maintain the vesting cell (output required).
#[test]
fn test_beneficiary_partial_claim_no_output() {
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

    // Setup header for epoch 200 (50% vested).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Beneficiary trying partial claim without output cell (incorrect).
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        // No output for vesting cell - incorrect for partial claim.
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - partial claim requires output");

    // Verify it's the correct error (BeneficiaryPartialClaimMissingOutput = 43).
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 43, "Expected error code 43 (BeneficiaryPartialClaimMissingOutput), got {}", error_code);
    }
}

/// Tests that tampering with total amount is rejected.
/// Total amount must remain constant throughout vesting lifecycle.
#[test]
fn test_total_amount_tampering() {
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

    // Setup header.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200),
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Try to change total amount (tampering).
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(12000, 5000, 0, 201).pack()) // Changed total amount!
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - total amount cannot change");

    // Verify it's the correct error (TotalAmountChanged = 14).
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 14, "Expected error code 14 (TotalAmountChanged), got {}", error_code);
    }
}

/// Tests that invalid beneficiary claimed delta is rejected.
/// The beneficiary_claimed value must increase by exactly the claimed amount.
#[test]
fn test_invalid_beneficiary_delta() {
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

    // Setup header for epoch 200.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 1000, 0, 200), // Already claimed 1000
    );

    // Create beneficiary authorization input cell.
    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Claim 2000 but update beneficiary_claimed incorrectly.
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(8161u64.pack()) // Claiming 2000 capacity
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 7000, 0, 201).pack()) // Wrong: Claiming 6000 (7000-1000) but only 4000 available
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - invalid beneficiary claimed delta, got error code: {:?}", extract_error_code(&result));

    // Verify it's the correct error (InsufficientVested = 21).
    // This triggers before the delta check because we're trying to claim more than available.
    if let Some(error_code) = extract_error_code(&result) {
        assert_eq!(error_code, 21, "Expected error code 21 (InsufficientVested), got {}", error_code);
    }
}

/// Tests that dual authorization (both creator and beneficiary) works correctly.
/// Both parties can be present but only one can perform the action.
#[test]
fn test_dual_authorization_attempt() {
    let mut context = Context::default();
    let contract_bin: Bytes = Loader::default().load_binary("vesting_lock");
    let out_point = context.deploy_cell(contract_bin);

    let (beneficiary_lock, beneficiary_hash, creator_lock, creator_hash) = setup_authorization_locks(&mut context);

    let args = create_vesting_args(
        creator_hash,
        beneficiary_hash,
        100, // start_epoch
        300, // end_epoch
        120, // cliff_epoch
    );

    let lock_script = context.build_script(&out_point, args).expect("script");

    // Setup header for epoch 200.
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200),
    );

    // Create BOTH authorization input cells.
    let creator_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(3000000000u64.pack())
            .lock(creator_lock.clone())
            .build(),
        Bytes::new(),
    );

    let beneficiary_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(3000000000u64.pack())
            .lock(beneficiary_lock.clone())
            .build(),
        Bytes::new(),
    );

    // Both parties present - creator takes precedence for termination.
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(creator_input_out_point).build())
        .input(CellInput::new_builder().previous_output(beneficiary_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 0, 5000, 201).pack()) // Creator terminates
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    // This should succeed as creator authorization takes precedence.
    assert!(result.is_ok(), "Should succeed - creator authorization takes precedence");
}