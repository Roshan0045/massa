// Copyright (c) 2022 MASSA LABS <info@massa.net>

use massa_hash::Hash;
use massa_models::config::CHAINID;
use massa_models::datastore::Datastore;
use massa_models::endorsement::EndorsementSerializer;
use massa_models::operation::{
    compute_operations_hash, OperationIdSerializer, OperationSerializer,
};
use massa_models::secure_share::SecureShareContent;
use massa_models::{
    address::Address,
    amount::Amount,
    block::{Block, BlockSerializer, SecureShareBlock},
    block_header::{BlockHeader, BlockHeaderSerializer},
    block_id::BlockId,
    endorsement::{Endorsement, SecureShareEndorsement},
    operation::{Operation, OperationType, SecureShareOperation},
    slot::Slot,
};
use massa_signature::KeyPair;

/// Creates a block for use in protocol,
/// without paying attention to consensus related things
/// like slot, parents, and merkle root.
pub fn create_block(keypair: &KeyPair) -> SecureShareBlock {
    let header = BlockHeader::new_verifiable(
        BlockHeader {
            current_version: 0,
            announced_version: None,
            slot: Slot::new(1, 0),
            parents: vec![
                BlockId::generate_from_hash(Hash::compute_from("Genesis 0".as_bytes())),
                BlockId::generate_from_hash(Hash::compute_from("Genesis 1".as_bytes())),
            ],
            operation_merkle_root: Hash::compute_from(&Vec::new()),
            endorsements: Vec::new(),
            denunciations: Vec::new(),
        },
        BlockHeaderSerializer::new(),
        keypair,
        *CHAINID,
    )
    .unwrap();

    Block::new_verifiable(
        Block {
            header,
            operations: Default::default(),
        },
        BlockSerializer::new(),
        keypair,
        *CHAINID,
    )
    .unwrap()
}

/// create a block with no endorsement
///
/// * `keypair`: key that sign the block
/// * `slot`
/// * `operations`
pub fn create_block_with_operations(
    keypair: &KeyPair,
    slot: Slot,
    operations: Vec<SecureShareOperation>,
) -> SecureShareBlock {
    let operation_merkle_root = compute_operations_hash(
        &operations.iter().map(|op| op.id).collect::<Vec<_>>(),
        &OperationIdSerializer::new(),
    );

    let header = BlockHeader::new_verifiable(
        BlockHeader {
            current_version: 0,
            announced_version: None,
            slot,
            parents: vec![
                BlockId::generate_from_hash(Hash::compute_from("Genesis 0".as_bytes())),
                BlockId::generate_from_hash(Hash::compute_from("Genesis 1".as_bytes())),
            ],
            operation_merkle_root,
            endorsements: Vec::new(),
            denunciations: Vec::new(),
        },
        BlockHeaderSerializer::new(),
        keypair,
        *CHAINID,
    )
    .unwrap();

    let op_ids = operations.into_iter().map(|op| op.id).collect();
    Block::new_verifiable(
        Block {
            header,
            operations: op_ids,
        },
        BlockSerializer::new(),
        keypair,
        *CHAINID,
    )
    .unwrap()
}

/// create a block with no operation
///
/// * `keypair`: key that sign the block
/// * `slot`
/// * `endorsements`
pub fn create_block_with_endorsements(
    keypair: &KeyPair,
    slot: Slot,
    endorsements: Vec<SecureShareEndorsement>,
) -> SecureShareBlock {
    let header = BlockHeader::new_verifiable(
        BlockHeader {
            current_version: 0,
            announced_version: None,
            slot,
            parents: vec![
                BlockId::generate_from_hash(Hash::compute_from("Genesis 0".as_bytes())),
                BlockId::generate_from_hash(Hash::compute_from("Genesis 1".as_bytes())),
            ],
            operation_merkle_root: Hash::compute_from(&Vec::new()),
            endorsements,
            denunciations: Vec::new(),
        },
        BlockHeaderSerializer::new(),
        keypair,
        *CHAINID,
    )
    .unwrap();

    Block::new_verifiable(
        Block {
            header,
            operations: Default::default(),
        },
        BlockSerializer::new(),
        keypair,
        *CHAINID,
    )
    .unwrap()
}

/// Creates an endorsement for use in protocol tests,
/// without paying attention to consensus related things.
pub fn create_endorsement() -> SecureShareEndorsement {
    let keypair = KeyPair::generate(0).unwrap();

    let content = Endorsement {
        slot: Slot::new(10, 1),
        index: 0,
        endorsed_block: BlockId::generate_from_hash(Hash::compute_from(&[])),
    };
    Endorsement::new_verifiable(content, EndorsementSerializer::new(), &keypair, *CHAINID).unwrap()
}

/// Create an operation, from a specific sender, and with a specific expire period.
pub fn create_operation_with_expire_period(
    keypair: &KeyPair,
    expire_period: u64,
) -> SecureShareOperation {
    let recv_keypair = KeyPair::generate(0).unwrap();

    let op = OperationType::Transaction {
        recipient_address: Address::from_public_key(&recv_keypair.get_public_key()),
        amount: Amount::default(),
    };
    let content = Operation {
        fee: Amount::default(),
        op,
        expire_period,
    };
    Operation::new_verifiable(content, OperationSerializer::new(), keypair, *CHAINID).unwrap()
}

/// Create an ExecuteSC operation with too much gas.
pub fn create_execute_sc_op_with_too_much_gas(
    keypair: &KeyPair,
    expire_period: u64,
) -> SecureShareOperation {
    let op = OperationType::ExecuteSC {
        data: Vec::new(),
        max_gas: (u32::MAX - 1) as u64,
        max_coins: Amount::default(),
        datastore: Datastore::default(),
    };
    let content = Operation {
        fee: Amount::default(),
        op,
        expire_period,
    };
    Operation::new_verifiable(content, OperationSerializer::new(), keypair, *CHAINID).unwrap()
}

/// Create a CallSC operation with too much gas.
pub fn create_call_sc_op_with_too_much_gas(
    keypair: &KeyPair,
    expire_period: u64,
) -> SecureShareOperation {
    use massa_models::address::{SCAddress, SCAddressV0};
    let sc_addr = Address::SC(SCAddress::SCAddressV0(SCAddressV0(
        massa_hash::Hash::compute_from("ADDR".as_bytes()),
    )));
    let op = OperationType::CallSC {
        target_addr: sc_addr,
        target_func: "some_fn".to_string(),
        param: Vec::new(),
        max_gas: (u32::MAX - 1) as u64,
        coins: Amount::default(),
    };
    let content = Operation {
        fee: Amount::default(),
        op,
        expire_period,
    };
    Operation::new_verifiable(content, OperationSerializer::new(), keypair, *CHAINID).unwrap()
}
