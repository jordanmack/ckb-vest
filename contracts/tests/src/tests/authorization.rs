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
    let wrong_out_point = context.deploy_cell(Bytes::new());
    let wrong_lock = context.build_script(&wrong_out_point, Bytes::from(vec![99])).expect("script");

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

    // Wrong authorization trying to claim
    let wrong_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(wrong_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(wrong_output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack())
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
    let wrong_out_point = context.deploy_cell(Bytes::new());
    let wrong_lock = context.build_script(&wrong_out_point, Bytes::from(vec![88])).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10161u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 2000, 0, 200),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    // Wrong authorization trying to terminate
    let wrong_output = CellOutput::new_builder()
        .capacity(3000u64.pack()) // unvested amount
        .lock(wrong_lock)
        .build();

    let remaining_output = CellOutput::new_builder()
        .capacity(7161u64.pack())
        .lock(lock_script)
        .build();

    let tx = TransactionBuilder::default()
        .input(input)
        .output(wrong_output)
        .output_data(Bytes::new().pack())
        .output(remaining_output)
        .output_data(create_vesting_data(10000, 2000, 3000, 201).pack())
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

    // Create lock with beneficiary_hash but use wrong lock script
    let wrong_out_point = context.deploy_cell(Bytes::new());
    let beneficiary_lock_wrong = context.build_script(&wrong_out_point, Bytes::from(vec![77])).expect("script");

    let input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(10000u64.pack())
            .lock(lock_script.clone())
            .build(),
        create_vesting_data(10000, 0, 0, 200),
    );

    let input = CellInput::new_builder()
        .previous_output(input_out_point)
        .build();

    let output = CellOutput::new_builder()
        .capacity(5000u64.pack())
        .lock(beneficiary_lock_wrong) // wrong lock script
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
        .output_data(create_vesting_data(10000, 5000, 0, 201).pack())
        .build();
    let tx = context.complete_tx(tx);

    let result = context.verify_tx(&tx, MAX_CYCLES);
    assert!(result.is_err(), "Should fail - wrong lock hash authorization, got error code: {:?}", extract_error_code(&result));
}