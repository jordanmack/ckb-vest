use super::helpers::*;
use crate::Loader;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_testtool::context::Context;

/// Tests that only authorized beneficiaries can claim vested tokens.
/// Ensures the contract rejects claims from wrong lock hash authorization.
#[test]
fn test_unauthorized_beneficiary_claim() {
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

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested
    );

    // Create WRONG authorization - use creator lock instead of beneficiary lock.
    let (wrong_lock, _wrong_hash) = create_always_success_lock_with_args(&mut context, vec![99u8]);
    let wrong_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(wrong_lock) // Wrong lock
            .build(),
        Bytes::new(),
    );

    // Try to claim without proper beneficiary authorization.
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(wrong_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script.clone())
            .build())
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack())
        .output(CellOutput::new_builder()
            .capacity(5000u64.pack())
            .lock(beneficiary_lock)
            .build())
        .output_data(Bytes::new().pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - unauthorized beneficiary claim, got error code: {:?}", extract_error_code(&result));
}

/// Tests that only authorized creators can terminate vesting schedules.
/// Validates proper authorization checking for termination operations.
#[test]
fn test_unauthorized_creator_termination() {
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

    // Setup header with block 201, higher than input's highest_block_seen (200).
    let header_hash = setup_header_with_block_and_epoch(&mut context, 201, 200);

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200), // 50% vested, 2000 claimed
    );

    // Create WRONG authorization - not the creator lock.
    let (wrong_lock, _wrong_hash) = create_always_success_lock_with_args(&mut context, vec![88u8]);
    let wrong_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(wrong_lock) // Wrong lock
            .build(),
        Bytes::new(),
    );

    // At epoch 200: vested = 5000, unvested = 5000 (creator should claim all 5000).
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(wrong_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 2000, 5000, 201).pack())
        .output(CellOutput::new_builder()
            .capacity(5000u64.pack())
            .lock(creator_lock)
            .build())
        .output_data(Bytes::new().pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - unauthorized creator termination, got error code: {:?}", extract_error_code(&result));
}

/// Tests that the contract properly validates lock hash authorization.
/// Ensures that claims are rejected when using incorrect lock scripts.
#[test]
fn test_wrong_lock_hash_authorization() {
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

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200), // 50% vested
    );

    // Create WRONG authorization - different lock that's not beneficiary or creator.
    let (wrong_lock, _wrong_hash) = create_always_success_lock_with_args(&mut context, vec![77u8]);
    let wrong_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(6100000000u64.pack())
            .lock(wrong_lock) // Wrong lock
            .build(),
        Bytes::new(),
    );

    // Try to claim with wrong authorization in inputs.
    let tx = TransactionBuilder::default()
        .input(CellInput::new_builder().previous_output(input_out_point).build())
        .input(CellInput::new_builder().previous_output(wrong_input_out_point).build())
        .output(CellOutput::new_builder()
            .capacity(5161u64.pack())
            .lock(lock_script)
            .build())
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack())
        .output(CellOutput::new_builder()
            .capacity(5000u64.pack())
            .lock(beneficiary_lock)
            .build())
        .output_data(Bytes::new().pack())
        .header_dep(header_hash)
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - wrong lock hash authorization, got error code: {:?}", extract_error_code(&result));
}