use massa_models::{
    block_header::{BlockHeader, BlockHeaderDeserializer, SecuredHeader},
    block_id::{BlockId, BlockIdDeserializer, BlockIdSerializer},
    operation::{
        OperationId, OperationIdSerializer, OperationIdsDeserializer, OperationsDeserializer,
        SecureShareOperation,
    },
    secure_share::{SecureShareDeserializer, SecureShareSerializer},
};
use massa_serialization::{Deserializer, Serializer, U64VarIntDeserializer, U64VarIntSerializer};
use nom::{
    error::{context, ContextError, ParseError},
    sequence::tuple,
    IResult, Parser,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::ops::Bound::Included;

/// Request block data
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AskForBlockInfo {
    /// Ask header
    Header,
    /// Ask for the list of operation IDs of the block
    #[default]
    OperationIds,
    /// Ask for a subset of operations of the block
    Operations(Vec<OperationId>),
}

/// Reply to a block data request
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum BlockInfoReply {
    /// Header
    Header(SecuredHeader),
    /// List of operation IDs within the block
    OperationIds(Vec<OperationId>),
    /// Requested full operations of the block
    Operations(Vec<SecureShareOperation>),
    /// Block not found
    NotFound,
}

#[derive(Debug)]
//TODO: Fix this clippy warning
#[allow(clippy::large_enum_variant)]
pub enum BlockMessage {
    /// Block header
    Header(SecuredHeader),
    /// Message asking the peer for info on a list of blocks.
    DataRequest {
        /// ID of the block to ask info for.
        block_id: BlockId,
        /// Block info to ask for.
        block_info: AskForBlockInfo,
    },
    /// Message replying with info on a list of blocks.
    DataResponse {
        /// ID of the block to reply info for.
        block_id: BlockId,
        /// Block info reply.
        block_info: BlockInfoReply,
    },
}

#[derive(IntoPrimitive, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u64)]
pub enum MessageTypeId {
    Header,
    DataRequest,
    DataResponse,
}

impl From<&BlockMessage> for MessageTypeId {
    fn from(value: &BlockMessage) -> Self {
        match value {
            BlockMessage::Header(_) => MessageTypeId::Header,
            BlockMessage::DataRequest { .. } => MessageTypeId::DataRequest,
            BlockMessage::DataResponse { .. } => MessageTypeId::DataResponse,
        }
    }
}

#[derive(IntoPrimitive, Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(u64)]
pub enum BlockInfoType {
    Header = 0,
    OperationIds = 1,
    Operations = 2,
    NotFound = 3,
}

#[derive(Default, Clone)]
pub struct BlockMessageSerializer {
    id_serializer: U64VarIntSerializer,
    secure_share_serializer: SecureShareSerializer,
    length_serializer: U64VarIntSerializer,
    block_id_serializer: BlockIdSerializer,
    operation_id_serializer: OperationIdSerializer,
}

impl BlockMessageSerializer {
    pub fn new() -> Self {
        Self {
            id_serializer: U64VarIntSerializer::new(),
            secure_share_serializer: SecureShareSerializer::new(),
            length_serializer: U64VarIntSerializer::new(),
            block_id_serializer: BlockIdSerializer::new(),
            operation_id_serializer: OperationIdSerializer::new(),
        }
    }
}

impl Serializer<BlockMessage> for BlockMessageSerializer {
    fn serialize(
        &self,
        value: &BlockMessage,
        buffer: &mut Vec<u8>,
    ) -> Result<(), massa_serialization::SerializeError> {
        self.id_serializer
            .serialize(&MessageTypeId::from(value).into(), buffer)?;
        match value {
            BlockMessage::Header(header) => {
                self.secure_share_serializer.serialize(header, buffer)?;
            }
            BlockMessage::DataRequest {
                block_id,
                block_info,
            } => {
                self.block_id_serializer.serialize(block_id, buffer)?;
                match block_info {
                    AskForBlockInfo::Header => {
                        self.id_serializer
                            .serialize(&(BlockInfoType::Header as u64), buffer)?;
                    }
                    AskForBlockInfo::OperationIds => {
                        self.id_serializer
                            .serialize(&(BlockInfoType::OperationIds as u64), buffer)?;
                    }
                    AskForBlockInfo::Operations(operations_ids) => {
                        self.id_serializer
                            .serialize(&(BlockInfoType::Operations as u64), buffer)?;
                        self.length_serializer
                            .serialize(&(operations_ids.len() as u64), buffer)?;
                        for operation_id in operations_ids {
                            self.operation_id_serializer
                                .serialize(operation_id, buffer)?;
                        }
                    }
                }
            }
            BlockMessage::DataResponse {
                block_id,
                block_info,
            } => {
                self.block_id_serializer.serialize(block_id, buffer)?;
                match block_info {
                    BlockInfoReply::Header(header) => {
                        self.id_serializer
                            .serialize(&(BlockInfoType::Header as u64), buffer)?;
                        self.secure_share_serializer.serialize(header, buffer)?;
                    }
                    BlockInfoReply::OperationIds(operations_ids) => {
                        self.id_serializer
                            .serialize(&(BlockInfoType::OperationIds as u64), buffer)?;
                        self.length_serializer
                            .serialize(&(operations_ids.len() as u64), buffer)?;
                        for operation_id in operations_ids {
                            self.operation_id_serializer
                                .serialize(operation_id, buffer)?;
                        }
                    }
                    BlockInfoReply::Operations(operations) => {
                        self.id_serializer
                            .serialize(&(BlockInfoType::Operations as u64), buffer)?;
                        self.length_serializer
                            .serialize(&(operations.len() as u64), buffer)?;
                        for operation in operations {
                            self.secure_share_serializer.serialize(operation, buffer)?;
                        }
                    }
                    BlockInfoReply::NotFound => {
                        self.id_serializer
                            .serialize(&(BlockInfoType::NotFound as u64), buffer)?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct BlockMessageDeserializer {
    id_deserializer: U64VarIntDeserializer,
    block_header_deserializer: SecureShareDeserializer<BlockHeader, BlockHeaderDeserializer>,
    block_id_deserializer: BlockIdDeserializer,
    operation_ids_deserializer: OperationIdsDeserializer,
    operations_deserializer: OperationsDeserializer,
}

pub struct BlockMessageDeserializerArgs {
    pub thread_count: u8,
    pub endorsement_count: u32,
    pub max_operations_per_block: u32,
    pub max_datastore_value_length: u64,
    pub max_function_name_length: u16,
    pub max_parameters_size: u32,
    pub max_op_datastore_entry_count: u64,
    pub max_op_datastore_key_length: u8,
    pub max_op_datastore_value_length: u64,
    pub max_denunciations_in_block_header: u32,
    pub last_start_period: Option<u64>,
    pub chain_id: u64,
}

impl BlockMessageDeserializer {
    pub fn new(args: BlockMessageDeserializerArgs) -> Self {
        Self {
            id_deserializer: U64VarIntDeserializer::new(Included(0), Included(u64::MAX)),
            block_header_deserializer: SecureShareDeserializer::new(
                BlockHeaderDeserializer::new(
                    args.thread_count,
                    args.endorsement_count,
                    args.max_denunciations_in_block_header,
                    args.last_start_period,
                    args.chain_id,
                ),
                args.chain_id,
            ),
            block_id_deserializer: BlockIdDeserializer::new(),
            operation_ids_deserializer: OperationIdsDeserializer::new(
                args.max_operations_per_block,
            ),
            operations_deserializer: OperationsDeserializer::new(
                args.max_operations_per_block,
                args.max_datastore_value_length,
                args.max_function_name_length,
                args.max_parameters_size,
                args.max_op_datastore_entry_count,
                args.max_op_datastore_key_length,
                args.max_op_datastore_value_length,
                args.chain_id,
            ),
        }
    }
}

impl Deserializer<BlockMessage> for BlockMessageDeserializer {
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], BlockMessage, E> {
        context("Failed BlockMessage deserialization", |buffer| {
            let (buffer, raw_id) = self.id_deserializer.deserialize(buffer)?;
            let id = MessageTypeId::try_from(raw_id).map_err(|_| {
                nom::Err::Error(ParseError::from_error_kind(
                    buffer,
                    nom::error::ErrorKind::Eof,
                ))
            })?;
            match id {
                MessageTypeId::Header => context("Failed BlockHeader deserialization", |input| {
                    self.block_header_deserializer.deserialize(input)
                })
                .map(BlockMessage::Header)
                .parse(buffer),
                MessageTypeId::DataRequest => context(
                    "Failed BlockDataRequest deserialization",
                    tuple((
                        context("Failed BlockId deserialization", |input| {
                            self.block_id_deserializer.deserialize(input)
                        }),
                        context("Failed infos deserialization", |input| {
                            let (rest, raw_id) = self.id_deserializer.deserialize(input)?;
                            let info_type: BlockInfoType = raw_id.try_into().map_err(|_| {
                                nom::Err::Error(ParseError::from_error_kind(
                                    buffer,
                                    nom::error::ErrorKind::Digit,
                                ))
                            })?;
                            match info_type {
                                BlockInfoType::Header => Ok((rest, AskForBlockInfo::Header)),
                                BlockInfoType::OperationIds => {
                                    Ok((rest, AskForBlockInfo::OperationIds))
                                }
                                BlockInfoType::Operations => self
                                    .operation_ids_deserializer
                                    .deserialize(rest)
                                    .map(|(rest, operation_ids)| {
                                        (rest, AskForBlockInfo::Operations(operation_ids))
                                    }),
                                BlockInfoType::NotFound => {
                                    Err(nom::Err::Error(ParseError::from_error_kind(
                                        buffer,
                                        nom::error::ErrorKind::Digit,
                                    )))
                                }
                            }
                        }),
                    )),
                )
                .map(|(block_id, block_info)| BlockMessage::DataRequest {
                    block_id,
                    block_info,
                })
                .parse(buffer),
                MessageTypeId::DataResponse => context(
                    "Failed BlockDataResponse deserialization",
                    tuple((
                        context("Failed BlockId deserialization", |input| {
                            self.block_id_deserializer.deserialize(input)
                        }),
                        context("Failed infos deserialization", |input| {
                            let (rest, raw_id) = self.id_deserializer.deserialize(input)?;
                            let info_type: BlockInfoType = raw_id.try_into().map_err(|_| {
                                nom::Err::Error(ParseError::from_error_kind(
                                    buffer,
                                    nom::error::ErrorKind::Digit,
                                ))
                            })?;
                            match info_type {
                                BlockInfoType::Header => self
                                    .block_header_deserializer
                                    .deserialize(rest)
                                    .map(|(rest, header)| (rest, BlockInfoReply::Header(header))),
                                BlockInfoType::OperationIds => self
                                    .operation_ids_deserializer
                                    .deserialize(rest)
                                    .map(|(rest, operation_ids)| {
                                        (rest, BlockInfoReply::OperationIds(operation_ids))
                                    }),
                                BlockInfoType::Operations => self
                                    .operations_deserializer
                                    .deserialize(rest)
                                    .map(|(rest, operations)| {
                                        (rest, BlockInfoReply::Operations(operations))
                                    }),
                                BlockInfoType::NotFound => Ok((rest, BlockInfoReply::NotFound)),
                            }
                        }),
                    )),
                )
                .map(|(block_id, block_info)| BlockMessage::DataResponse {
                    block_id,
                    block_info,
                })
                .parse(buffer),
            }
        })
        .parse(buffer)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use massa_models::config::CHAINID;
    use massa_models::{block_id::BlockId, operation::OperationId};
    use massa_serialization::{DeserializeError, Deserializer, Serializer};

    #[test]
    fn test_lower_limit_message() {
        let message = super::BlockMessage::DataRequest {
            block_id: BlockId::from_str("B12DvrcQkzF1Wi8BVoNfc4n93CD3E2qhCNe7nVhnEQGWHZ24fEmg")
                .unwrap(),
            block_info: super::AskForBlockInfo::Operations(vec![]),
        };
        let mut buffer = Vec::new();
        let serializer = super::BlockMessageSerializer::new();
        serializer.serialize(&message, &mut buffer).unwrap();
        let deserializer =
            super::BlockMessageDeserializer::new(super::BlockMessageDeserializerArgs {
                thread_count: 1,
                endorsement_count: 1,
                max_operations_per_block: 1,
                max_datastore_value_length: 1,
                max_function_name_length: 1,
                max_parameters_size: 1,
                max_op_datastore_entry_count: 1,
                max_op_datastore_key_length: 1,
                max_op_datastore_value_length: 1,
                max_denunciations_in_block_header: 1,
                last_start_period: None,
                chain_id: *CHAINID,
            });
        let (rest, deserialized_message) = deserializer
            .deserialize::<DeserializeError>(&buffer)
            .unwrap();
        assert!(rest.is_empty());
        match (deserialized_message, message) {
            (
                super::BlockMessage::DataRequest {
                    block_id: block_id1,
                    block_info: block_info1,
                },
                super::BlockMessage::DataRequest {
                    block_id: block_id2,
                    block_info: block_info2,
                },
            ) => {
                assert_eq!(block_id1, block_id2);
                assert_eq!(block_info1, block_info2);
            }
            _ => panic!("Wrong message type"),
        }
        let message2 = super::BlockMessage::DataResponse {
            block_id: BlockId::from_str("B12DvrcQkzF1Wi8BVoNfc4n93CD3E2qhCNe7nVhnEQGWHZ24fEmg")
                .unwrap(),
            block_info: super::BlockInfoReply::Operations(vec![]),
        };
        let mut buffer2 = Vec::new();
        serializer.serialize(&message2, &mut buffer2).unwrap();
        let (rest2, deserialized_message2) = deserializer
            .deserialize::<DeserializeError>(&buffer2)
            .unwrap();
        assert!(rest2.is_empty());
        match (deserialized_message2, message2) {
            (
                super::BlockMessage::DataResponse {
                    block_id: block_id1,
                    block_info: block_info1,
                },
                super::BlockMessage::DataResponse {
                    block_id: block_id2,
                    block_info: block_info2,
                },
            ) => {
                assert_eq!(block_id1, block_id2);
                match (block_info1, block_info2) {
                    (
                        super::BlockInfoReply::Operations(operations1),
                        super::BlockInfoReply::Operations(operations2),
                    ) => {
                        assert_eq!(operations1.len(), operations2.len());
                    }
                    _ => panic!("Wrong block info type"),
                }
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_high_limit_message() {
        let message = super::BlockMessage::DataRequest {
            block_id: BlockId::from_str("B12DvrcQkzF1Wi8BVoNfc4n93CD3E2qhCNe7nVhnEQGWHZ24fEmg")
                .unwrap(),
            block_info: super::AskForBlockInfo::Operations(vec![
                OperationId::from_str("O1yrsTtyyhDJtPD7jZHkodstNCjUSsfGbVZ5xdG6bVZWABeze8y")
                    .unwrap(),
                OperationId::from_str("O1yrsTtyyhDJtPD7jZHkodstNCjUSsfGbVZ5xdG6bVZWABeze8y")
                    .unwrap(),
            ]),
        };
        let mut buffer = Vec::new();
        let serializer = super::BlockMessageSerializer::new();
        serializer.serialize(&message, &mut buffer).unwrap();
        let deserializer =
            super::BlockMessageDeserializer::new(super::BlockMessageDeserializerArgs {
                thread_count: 1,
                endorsement_count: 1,
                max_operations_per_block: 1,
                max_datastore_value_length: 1,
                max_function_name_length: 1,
                max_parameters_size: 1,
                max_op_datastore_entry_count: 1,
                max_op_datastore_key_length: 1,
                max_op_datastore_value_length: 1,
                max_denunciations_in_block_header: 1,
                last_start_period: None,
                chain_id: *CHAINID,
            });
        deserializer
            .deserialize::<DeserializeError>(&buffer)
            .expect_err("Should raise error because there is two op and only 1 allow");
        let deserializer =
            super::BlockMessageDeserializer::new(super::BlockMessageDeserializerArgs {
                thread_count: 1,
                endorsement_count: 1,
                max_operations_per_block: 2,
                max_datastore_value_length: 1,
                max_function_name_length: 1,
                max_parameters_size: 1,
                max_op_datastore_entry_count: 1,
                max_op_datastore_key_length: 1,
                max_op_datastore_value_length: 1,
                max_denunciations_in_block_header: 1,
                last_start_period: None,
                chain_id: *CHAINID,
            });
        let (rest, deserialized_message) = deserializer
            .deserialize::<DeserializeError>(&buffer)
            .unwrap();
        assert!(rest.is_empty());
        match (deserialized_message, message) {
            (
                super::BlockMessage::DataRequest {
                    block_id: block_id1,
                    block_info: block_info1,
                },
                super::BlockMessage::DataRequest {
                    block_id: block_id2,
                    block_info: block_info2,
                },
            ) => {
                assert_eq!(block_id1, block_id2);
                assert_eq!(block_info1, block_info2);
            }
            _ => panic!("Wrong message type"),
        }
    }
}
