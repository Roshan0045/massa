// Copyright (c) 2022 MASSA LABS <info@massa.net>

use crate::error::ModelsError;
use crate::prehash::PreHashed;
use massa_hash::{Hash, HashDeserializer, HASH_SIZE_BYTES};
use massa_serialization::{
    DeserializeError, Deserializer, SerializeError, Serializer, U64VarIntDeserializer,
    U64VarIntSerializer,
};
use massa_signature::{PublicKey, PublicKeyV0};
use nom::error::{context, ContextError, ErrorKind, ParseError};
use nom::{IResult, Parser};
use serde::{Deserialize, Serialize};
use std::ops::Bound::{Excluded, Included};
use std::str::FromStr;
use transition::Versioned;

/// Top level address representation that can differentiate between User and SC address
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Address {
    #[allow(missing_docs)]
    User(UserAddress),
    #[allow(missing_docs)]
    SC(SCAddress),
}

#[allow(missing_docs)]
/// Derived from a public key.
#[transition::versioned(versions("0"))]
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SCAddress(pub Hash);

#[allow(missing_docs)]
/// Derived from a public key.
#[transition::versioned(versions("0"))]
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UserAddress(pub Hash);

const ADDRESS_PREFIX: char = 'A';
// serialized with varint
const USER_PREFIX: u64 = 0;
const SC_PREFIX: u64 = 1;

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Address::User(address) => address.fmt(f),
            Address::SC(address) => address.fmt(f),
        }
    }
}

impl std::fmt::Display for UserAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            UserAddress::UserAddressV0(address) => address.fmt(f),
        }
    }
}

impl std::fmt::Display for SCAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SCAddress::SCAddressV0(address) => address.fmt(f),
        }
    }
}

#[transition::impl_version(versions("0"))]
impl std::fmt::Display for UserAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let u64_serializer = U64VarIntSerializer::new();
        let mut bytes: Vec<u8> = Vec::new();
        u64_serializer
            .serialize(&Self::VERSION, &mut bytes)
            .map_err(|_| std::fmt::Error)?;
        bytes.extend(self.0.to_bytes());
        write!(
            f,
            "{}U{}",
            ADDRESS_PREFIX,
            bs58::encode(bytes).with_check().into_string()
        )
    }
}

#[transition::impl_version(versions("0"))]
impl std::fmt::Display for SCAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let u64_serializer = U64VarIntSerializer::new();
        let mut bytes: Vec<u8> = Vec::new();
        u64_serializer
            .serialize(&Self::VERSION, &mut bytes)
            .map_err(|_| std::fmt::Error)?;
        bytes.extend(self.0.to_bytes());
        write!(
            f,
            "{}S{}",
            ADDRESS_PREFIX,
            bs58::encode(bytes).with_check().into_string()
        )
    }
}

// See https://github.com/massalabs/massa/pull/3479#issuecomment-1408694720
// as to why more information is not provided
impl std::fmt::Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl ::serde::Serialize for Address {
    fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Address::User(address) => address.serialize(s),
            Address::SC(address) => address.serialize(s),
        }
    }
}

impl ::serde::Serialize for UserAddress {
    fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            UserAddress::UserAddressV0(address) => address.serialize(s),
        }
    }
}

impl ::serde::Serialize for SCAddress {
    fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            SCAddress::SCAddressV0(address) => address.serialize(s),
        }
    }
}

#[transition::impl_version(versions("0"))]
impl ::serde::Serialize for UserAddress {
    fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.collect_str(&self.to_string())
        } else {
            s.serialize_bytes(&self.to_prefixed_bytes())
        }
    }
}

#[transition::impl_version(versions("0"))]
impl ::serde::Serialize for SCAddress {
    fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.collect_str(&self.to_string())
        } else {
            s.serialize_bytes(&self.to_prefixed_bytes())
        }
    }
}

impl<'de> ::serde::Deserialize<'de> for Address {
    fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<Address, D::Error> {
        if d.is_human_readable() {
            struct AddressVisitor;

            impl<'de> ::serde::de::Visitor<'de> for AddressVisitor {
                type Value = Address;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("A + {U | S} + base58::encode(version + hash)")
                }

                fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
                where
                    E: ::serde::de::Error,
                {
                    if let Ok(v_str) = std::str::from_utf8(v) {
                        Address::from_str(v_str).map_err(E::custom)
                    } else {
                        Err(E::invalid_value(::serde::de::Unexpected::Bytes(v), &self))
                    }
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: ::serde::de::Error,
                {
                    Address::from_str(v).map_err(E::custom)
                }
            }
            d.deserialize_str(AddressVisitor)
        } else {
            struct BytesVisitor;

            impl<'de> ::serde::de::Visitor<'de> for BytesVisitor {
                type Value = Address;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("[u64varint-of-addr-variant][u64varint-of-version][bytes]")
                }

                fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
                where
                    E: ::serde::de::Error,
                {
                    AddressDeserializer::new()
                        .deserialize::<DeserializeError>(v)
                        .map_err(E::custom)
                        .map(|r| r.1)
                }
            }

            d.deserialize_bytes(BytesVisitor)
        }
    }
}

impl FromStr for Address {
    type Err = ModelsError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = Err(ModelsError::AddressParseError(s.to_string()));

        // Handle the prefix ("A{U|S}")
        let mut chars = s.chars();
        let Some(ADDRESS_PREFIX) = chars.next() else {
            return err;
        };
        let Some(pref) = chars.next() else {
            return err;
        };

        let res = match pref {
            'U' => Address::User(UserAddress::from_str_without_prefixed_type(chars.as_str())?),
            'S' => Address::SC(SCAddress::from_str_without_prefixed_type(chars.as_str())?),
            _ => return err,
        };
        Ok(res)
    }
}

impl Address {
    /// Gets the associated thread. Depends on the `thread_count`
    /// Returns None for SC addresses, even though we may want to get_thread on them in the future
    pub fn get_thread(&self, thread_count: u8) -> u8 {
        match self {
            Address::User(addr) => addr.get_thread(thread_count),
            // TODO: tmp behaviour, discuss how we would want this to work
            Address::SC(_addr) => 0,
        }
    }

    /// Computes the address associated with the given public key.
    /// Depends on the Public Key version
    pub fn from_public_key(public_key: &PublicKey) -> Self {
        Address::User(UserAddress::from_public_key(public_key))
    }

    /// Serialize the address as bytes. Includes the type and version prefixes
    pub fn to_prefixed_bytes(self) -> Vec<u8> {
        match self {
            Address::User(addr) => addr.to_prefixed_bytes(),
            Address::SC(addr) => addr.to_prefixed_bytes(),
        }
    }
}

impl UserAddress {
    /// Gets the associated thread. Depends on the `thread_count`
    fn get_thread(&self, thread_count: u8) -> u8 {
        match self {
            UserAddress::UserAddressV0(addr) => addr.get_thread(thread_count),
        }
    }

    /// Computes the address associated with the given public key
    fn from_public_key(public_key: &PublicKey) -> Self {
        match public_key {
            PublicKey::PublicKeyV0(pk) => {
                UserAddressVariant!["0"](<UserAddress!["0"]>::from_public_key(pk))
            }
        }
    }

    fn from_str_without_prefixed_type(s: &str) -> Result<Self, ModelsError> {
        let decoded_bs58_check = bs58::decode(s).with_check(None).into_vec().map_err(|err| {
            ModelsError::AddressParseError(format!(
                "in UserAddress from_str_without_prefixed_type: {}",
                err
            ))
        })?;
        let u64_deserializer = U64VarIntDeserializer::new(Included(0), Included(u64::MAX));
        let (rest, version) = u64_deserializer
            .deserialize::<DeserializeError>(&decoded_bs58_check[..])
            .map_err(|err| {
                ModelsError::AddressParseError(format!(
                    "in UserAddress from_str_without_prefixed_type: {}",
                    err
                ))
            })?;

        match version {
            <UserAddress!["0"]>::VERSION => Ok(UserAddressVariant!["0"](
                <UserAddress!["0"]>::from_bytes(rest)?,
            )),
            unhandled_version => Err(ModelsError::AddressParseError(format!(
                "version {} is not handled for UserAddress",
                unhandled_version
            ))),
        }
    }

    /// Serialize the address as bytes. Includes the type and version prefixes
    pub fn to_prefixed_bytes(self) -> Vec<u8> {
        match self {
            UserAddress::UserAddressV0(addr) => addr.to_prefixed_bytes(),
        }
    }
}

#[transition::impl_version(versions("0"))]
impl UserAddress {
    /// Fetches the version of the UserAddress
    pub fn get_version(&self) -> u64 {
        Self::VERSION
    }

    /// Serialize the address as bytes. Includes the type and version prefixes
    fn to_prefixed_bytes(self) -> Vec<u8> {
        let mut buff = vec![];
        let addr_type_ser = U64VarIntSerializer::new();
        let addr_vers_ser = U64VarIntSerializer::new();
        addr_type_ser
            .serialize(&USER_PREFIX, &mut buff)
            .expect("impl always returns Ok(())");
        addr_vers_ser
            .serialize(&Self::VERSION, &mut buff)
            .expect("impl always returns Ok(())");
        buff.extend_from_slice(&self.0.to_bytes()[..]);
        buff
    }

    /// Gets the associated thread. Depends on the `thread_count`
    fn get_thread(&self, thread_count: u8) -> u8 {
        (self.0.to_bytes()[0])
            .checked_shr(8 - thread_count.trailing_zeros())
            .unwrap_or(0)
    }

    /// Deserialize the address without considering the version byte
    fn from_bytes(data: &[u8]) -> Result<UserAddress, ModelsError> {
        Ok(UserAddress(Hash::from_bytes(&data.try_into().map_err(
            |_| {
                ModelsError::BufferError(format!(
                    "expected a buffer of size {}, but found a size of {}",
                    HASH_SIZE_BYTES,
                    &data.len()
                ))
            },
        )?)))
    }
}

#[transition::impl_version(versions("0"), structures("UserAddress", "PublicKey"))]
impl UserAddress {
    /// Computes address associated with given public key
    pub fn from_public_key(public_key: &PublicKey) -> Self {
        UserAddress(Hash::compute_from(&public_key.to_bytes()))
    }
}

#[transition::impl_version(versions("0"))]
impl UserAddress {}

impl SCAddress {
    fn from_str_without_prefixed_type(s: &str) -> Result<Self, ModelsError> {
        let decoded_bs58_check = bs58::decode(s).with_check(None).into_vec().map_err(|err| {
            ModelsError::AddressParseError(format!(
                "in SCAddress from_str_without_prefixed_type: {}",
                err
            ))
        })?;
        let u64_deserializer = U64VarIntDeserializer::new(Included(0), Included(u64::MAX));
        let (rest, version) = u64_deserializer
            .deserialize::<DeserializeError>(&decoded_bs58_check[..])
            .map_err(|err| {
                ModelsError::AddressParseError(format!(
                    "in SCAddress from_str_without_prefixed_type: {}",
                    err
                ))
            })?;

        match version {
            <SCAddress!["0"]>::VERSION => {
                Ok(SCAddressVariant!["0"](<SCAddress!["0"]>::from_bytes(rest)?))
            }
            unhandled_version => Err(ModelsError::AddressParseError(format!(
                "version {} is not handled for SCAddress",
                unhandled_version
            ))),
        }
    }

    /// Serialize the address as bytes. Includes the type and version prefixes
    pub fn to_prefixed_bytes(self) -> Vec<u8> {
        match self {
            SCAddress::SCAddressV0(addr) => addr.to_prefixed_bytes(),
        }
    }
}

#[transition::impl_version(versions("0"))]
impl SCAddress {
    /// Fetches the version of the SC Address
    pub fn get_version(&self) -> u64 {
        Self::VERSION
    }
}

#[transition::impl_version(versions("0"))]
impl SCAddress {
    /// Serialize the address as bytes. Includes the type and version prefixes
    pub fn to_prefixed_bytes(self) -> Vec<u8> {
        let mut buff = vec![];
        let addr_type_ser = U64VarIntSerializer::new();
        let addr_vers_ser = U64VarIntSerializer::new();
        addr_type_ser
            .serialize(&SC_PREFIX, &mut buff)
            .expect("impl always returns Ok(())");
        addr_vers_ser
            .serialize(&Self::VERSION, &mut buff)
            .expect("impl always returns Ok(())");
        buff.extend_from_slice(&self.0.to_bytes()[..]);
        buff
    }

    /// Deserialize the address without considering the version byte
    fn from_bytes(data: &[u8]) -> Result<SCAddress, ModelsError> {
        Ok(SCAddress(Hash::from_bytes(&data.try_into().map_err(
            |_| {
                ModelsError::BufferError(format!(
                    "expected a buffer of size {}, but found a size of {}",
                    HASH_SIZE_BYTES,
                    &data.len()
                ))
            },
        )?)))
    }
}

/* /!\ SCAddressV1 not prehashed! */
impl PreHashed for Address {}

/// Serializer for `Address`
#[derive(Default, Clone)]
pub struct AddressSerializer {
    type_serializer: U64VarIntSerializer,
    version_serializer: U64VarIntSerializer,
}

impl AddressSerializer {
    /// Serializes an `Address` into a `Vec<u8>`
    pub fn new() -> Self {
        Self {
            type_serializer: U64VarIntSerializer::new(),
            version_serializer: U64VarIntSerializer::new(),
        }
    }
}

impl Serializer<Address> for AddressSerializer {
    fn serialize(&self, value: &Address, buffer: &mut Vec<u8>) -> Result<(), SerializeError> {
        match value {
            Address::User(addr) => self.serialize(addr, buffer),
            Address::SC(addr) => self.serialize(addr, buffer),
        }
    }
}

impl Serializer<UserAddress> for AddressSerializer {
    fn serialize(&self, value: &UserAddress, buffer: &mut Vec<u8>) -> Result<(), SerializeError> {
        self.type_serializer.serialize(&USER_PREFIX, buffer)?;
        match value {
            UserAddress::UserAddressV0(addr) => self.serialize(addr, buffer),
        }
    }
}

#[transition::impl_version(versions("0"), structures("UserAddress"))]
impl Serializer<UserAddress> for AddressSerializer {
    fn serialize(&self, value: &UserAddress, buffer: &mut Vec<u8>) -> Result<(), SerializeError> {
        self.version_serializer
            .serialize(&value.get_version(), buffer)?;
        buffer.extend_from_slice(&value.0.into_bytes());
        Ok(())
    }
}

impl Serializer<SCAddress> for AddressSerializer {
    fn serialize(&self, value: &SCAddress, buffer: &mut Vec<u8>) -> Result<(), SerializeError> {
        self.type_serializer.serialize(&SC_PREFIX, buffer)?;
        match value {
            SCAddress::SCAddressV0(addr) => self.serialize(addr, buffer),
        }
    }
}

#[transition::impl_version(versions("0"), structures("SCAddress"))]
impl Serializer<SCAddress> for AddressSerializer {
    fn serialize(&self, value: &SCAddress, buffer: &mut Vec<u8>) -> Result<(), SerializeError> {
        self.version_serializer
            .serialize(&value.get_version(), buffer)?;
        buffer.extend_from_slice(&value.0.into_bytes());
        Ok(())
    }
}

/// Deserializer for `Address`
#[derive(Clone)]
pub struct AddressDeserializer {
    type_deserializer: U64VarIntDeserializer,
    version_deserializer: U64VarIntDeserializer,
    hash_deserializer: HashDeserializer,
}

impl Default for AddressDeserializer {
    fn default() -> Self {
        AddressDeserializer::new()
    }
}

impl AddressDeserializer {
    /// Creates a new deserializer for `Address`
    pub const fn new() -> Self {
        Self {
            type_deserializer: U64VarIntDeserializer::new(Included(0), Included(1)),
            version_deserializer: U64VarIntDeserializer::new(Included(0), Excluded(u64::MAX)),
            hash_deserializer: HashDeserializer::new(),
        }
    }
}

impl Deserializer<Address> for AddressDeserializer {
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], Address, E> {
        // Verify that we at least have a version and something else
        if buffer.len() < 2 {
            return Err(nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof)));
        }
        let (rest, addr_type) =
            self.type_deserializer
                .deserialize(buffer)
                .map_err(|_: nom::Err<E>| {
                    nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof))
                })?;
        match addr_type {
            USER_PREFIX => {
                let (rest, addr) = self.deserialize(rest)?;
                Ok((rest, Address::User(addr)))
            }
            SC_PREFIX => {
                let (rest, addr) = self.deserialize(rest)?;
                Ok((rest, Address::SC(addr)))
            }
            _ => Err(nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof))),
        }
    }
}

impl Deserializer<UserAddress> for AddressDeserializer {
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], UserAddress, E> {
        // Verify that we at least have a version and something else
        if buffer.len() < 2 {
            return Err(nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof)));
        }
        let (rest, addr_vers) =
            self.version_deserializer
                .deserialize(buffer)
                .map_err(|_: nom::Err<E>| {
                    nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof))
                })?;
        match addr_vers {
            <UserAddress!["0"]>::VERSION => {
                let (rest, addr) = self.deserialize(rest)?;
                Ok((rest, UserAddressVariant!["0"](addr)))
            }
            _ => Err(nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof))),
        }
    }
}

#[transition::impl_version(versions("0"), structures("UserAddress"))]
impl Deserializer<UserAddress> for AddressDeserializer {
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], UserAddress, E> {
        context("Failed UserAddress deserialization", |input| {
            self.hash_deserializer.deserialize(input)
        })
        .map(UserAddress)
        .parse(buffer)
    }
}

impl Deserializer<SCAddress> for AddressDeserializer {
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], SCAddress, E> {
        // Verify that we at least have a version and something else
        if buffer.len() < 2 {
            return Err(nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof)));
        }
        let (rest, addr_vers) =
            self.version_deserializer
                .deserialize(buffer)
                .map_err(|_: nom::Err<E>| {
                    nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof))
                })?;
        match addr_vers {
            <SCAddress!["0"]>::VERSION => {
                let (rest, addr) = self.deserialize(rest)?;
                Ok((rest, SCAddressVariant!["0"](addr)))
            }
            _ => Err(nom::Err::Error(E::from_error_kind(buffer, ErrorKind::Eof))),
        }
    }
}

#[transition::impl_version(versions("0"), structures("SCAddress"))]
impl Deserializer<SCAddress> for AddressDeserializer {
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], SCAddress, E> {
        context("Failed SCAddress deserialization", |input| {
            self.hash_deserializer.deserialize(input)
        })
        .map(SCAddress)
        .parse(buffer)
    }
}

/// Info for a given address on a given cycle
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionAddressCycleInfo {
    /// cycle number
    pub cycle: u64,
    /// true if that cycle is final
    pub is_final: bool,
    /// `ok_count` blocks were created by this address during that cycle
    pub ok_count: u64,
    /// `ok_count` blocks were missed by this address during that cycle
    pub nok_count: u64,
    /// number of active rolls the address had at that cycle (if still available)
    pub active_rolls: Option<u64>,
}

#[cfg(test)]
mod test {
    use crate::config::THREAD_COUNT;

    use super::*;

    #[test]
    fn test_address() {
        let expected_user_addr =
            Address::from_str("AU12fZLkHnLED3okr8Lduyty7dz9ZKkd24xMCc2JJWPcdmfn2eUEx").unwrap();
        let expected_sc_addr =
            Address::from_str("AS12fZLkHnLED3okr8Lduyty7dz9ZKkd24xMCc2JJWPcdmfn2eUEx").unwrap();

        let hash = massa_hash::Hash::compute_from("ADDR".as_bytes());
        let actual_user_addr = Address::User(UserAddress::UserAddressV0(UserAddressV0(hash)));
        let actual_sc_addr = Address::SC(SCAddress::SCAddressV0(SCAddressV0(hash)));

        assert_eq!(actual_user_addr, expected_user_addr);
        assert_eq!(actual_sc_addr, expected_sc_addr);
    }

    #[test]
    fn test_address_errors() {
        let expected_error_0 = "address parsing error: UnexpectedAddress".to_string();
        let actual_error_0 = Address::from_str("UnexpectedAddress")
            .unwrap_err()
            .to_string();

        let expected_error_1 = "address parsing error: in SCAddress from_str_without_prefixed_type: invalid checksum, calculated checksum: '[255, 246, 254, 58]', expected checksum: [188, 126, 221, 98]".to_string();
        let actual_error_1 = Address::from_str("ASSomeUnexpectedAddress")
            .unwrap_err()
            .to_string();

        let expected_error_2 = "address parsing error: in SCAddress from_str_without_prefixed_type: provided string contained invalid character '_' at byte 0".to_string();
        let actual_error_2 = Address::from_str("AS_SomeUnexpectedAddress")
            .unwrap_err()
            .to_string();

        let expected_error_3 = "address parsing error: in UserAddress from_str_without_prefixed_type: invalid checksum, calculated checksum: '[255, 246, 254, 58]', expected checksum: [188, 126, 221, 98]".to_string();
        let actual_error_3 = Address::from_str("AUSomeUnexpectedAddress")
            .unwrap_err()
            .to_string();

        let expected_error_4 = "address parsing error: in UserAddress from_str_without_prefixed_type: provided string contained invalid character '_' at byte 0".to_string();
        let actual_error_4 = Address::from_str("AU_SomeUnexpectedAddress")
            .unwrap_err()
            .to_string();

        assert_eq!(actual_error_0, expected_error_0);
        assert_eq!(actual_error_1, expected_error_1);
        assert_eq!(actual_error_2, expected_error_2);
        assert_eq!(actual_error_3, expected_error_3);
        assert_eq!(actual_error_4, expected_error_4);
    }

    #[test]
    fn test_address_get_thread() {
        let hash = massa_hash::Hash::compute_from("ADDR".as_bytes());

        let user_addr_0 = Address::User(UserAddress::UserAddressV0(UserAddressV0(hash)));
        let thread_addr_0 = user_addr_0.get_thread(THREAD_COUNT);
        let hash = massa_hash::Hash::compute_from("ADDR2".as_bytes());

        let user_addr_1 = Address::User(UserAddress::UserAddressV0(UserAddressV0(hash)));
        let thread_addr_1 = user_addr_1.get_thread(THREAD_COUNT);

        assert_ne!(thread_addr_0, thread_addr_1);
    }

    #[test]
    fn test_address_serde() {
        let expected_addr =
            Address::from_str("AU12fZLkHnLED3okr8Lduyty7dz9ZKkd24xMCc2JJWPcdmfn2eUEx").unwrap();

        let serialized = serde_json::to_string(&expected_addr).unwrap();
        let actual_addr: Address = serde_json::from_str(&serialized).unwrap();

        assert_eq!(actual_addr, expected_addr);
    }
}
