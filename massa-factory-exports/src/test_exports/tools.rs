use massa_hash::Hash;
use massa_models::config::CHAINID;
use massa_models::{
    block::{Block, BlockSerializer, SecureShareBlock},
    block_header::{BlockHeader, BlockHeaderSerializer},
    secure_share::SecureShareContent,
    slot::Slot,
};
use massa_signature::KeyPair;

/// Create an empty block for testing. Can be used to generate genesis blocks.
pub fn create_empty_block(keypair: &KeyPair, slot: &Slot) -> SecureShareBlock {
    let header = BlockHeader::new_verifiable(
        BlockHeader {
            current_version: 0,
            announced_version: None,
            slot: *slot,
            parents: Vec::new(),
            operation_merkle_root: Hash::compute_from(&Vec::new()),
            endorsements: Vec::new(),
            denunciations: vec![],
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
