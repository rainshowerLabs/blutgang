//! Available RPC methods and impls.

use std::fmt;

#[derive(Debug, thiserror::Error)]
#[error("failed to convert method to `EthRpcMethod`:\n\ngot: {0:?}\nexpected:\n{1:#?}")]
pub struct Error<Method>(Method, &'static [&'static str])
where
    Method: fmt::Debug;
impl<Method> Error<Method>
where
    Method: fmt::Debug,
{
    pub fn new(method: Method) -> Self {
        Self(method, EthRpcMethod::ETH_ALL)
    }
}

/// A list of RPC methods.
///
/// Each method has a corresponding `'static str` value associated with it
/// available via `std::convert::AsRef<str>`, `std::fmt::Display`, `serde::Serialize`
/// and `Self::as_str` impls for convenience.
///
/// # Example
///
/// ```rs,ignore
/// use crate::rpc::method::EthRpcMethod;
///
/// let method = EthRpcMethod::BlockNumber;
///
/// // `std::convert::AsRef<str>`
/// assert_eq!(method.as_ref(), "eth_blockNumber");
///
/// // `std::fmt::Display`
/// tracing::info!("Calling method: {method}");
///
/// // `serde::Serialize`
/// assert_eq!(serde_json::to_string(&method).unwrap(), "\"eth_blockNumber\"");
///
/// // `Self::as_str` for circumventing lifetimes associated with `let` bindings
/// rust_tracing::deps::metrics::gauge!("rpc_requests_active", "method" => method.as_str()).increment(1);
/// ```
#[derive(Debug, Clone, Copy)]
pub enum EthRpcMethod {
    BlockNumber,
    GetBlockByNumber,
    Syncing,
    GetBalance,
    GetStorageAt,
    GetTransactionCount,
    GetBlockTransactionCountByNumber,
    GetUncleCountByBlockNumber,
    GetCode,
    Call,
    GetTransactionByBlockNumberAndIndex,
    GetUncleByBlockNumberAndIndex,
    Subscribe,
    Unsubscribe,
    Subscription,
}
impl EthRpcMethod {
    const ETH_BLOCK_NUMBER: &str = "eth_blockNumber";
    const ETH_GET_BLOCK_BY_NUMBER: &str = "eth_getBlockByNumber";
    const ETH_SYNCING: &str = "eth_syncing";
    const ETH_GET_BALANCE: &str = "eth_getBalance";
    const ETH_GET_STORAGE_AT: &str = "eth_getStorageAt";
    const ETH_GET_TRANSACTION_COUNT: &str = "eth_getTransactionCount";
    const ETH_GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER: &str = "eth_getBlockTransactionCountByNumber";
    const ETH_GET_UNCLE_COUNT_BY_BLOCK_NUMBER: &str = "eth_getUncleCountByBlockNumber";
    const ETH_GET_CODE: &str = "eth_getCode";
    const ETH_CALL: &str = "eth_call";
    const ETH_GET_TRANSACTION_BY_BLOCK_NUMBER_AND_INDEX: &str =
        "eth_getTransactionByBlockNumberAndIndex";
    const ETH_GET_UNCLE_BY_BLOCK_NUMBER_AND_INDEX: &str = "eth_getUncleByBlockNumberAndIndex";
    const ETH_SUBSCRIBE: &str = "eth_subscribe";
    const ETH_UNSUBSCRIBE: &str = "eth_unsubscribe";
    const ETH_SUBSCRIPTION: &str = "eth_subscription";

    const ETH_ALL: &[&str; 15] = &[
        Self::ETH_BLOCK_NUMBER,
        Self::ETH_GET_BLOCK_BY_NUMBER,
        Self::ETH_SYNCING,
        Self::ETH_GET_BALANCE,
        Self::ETH_GET_STORAGE_AT,
        Self::ETH_GET_TRANSACTION_COUNT,
        Self::ETH_GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER,
        Self::ETH_GET_UNCLE_COUNT_BY_BLOCK_NUMBER,
        Self::ETH_GET_CODE,
        Self::ETH_CALL,
        Self::ETH_GET_TRANSACTION_BY_BLOCK_NUMBER_AND_INDEX,
        Self::ETH_GET_UNCLE_BY_BLOCK_NUMBER_AND_INDEX,
        Self::ETH_SUBSCRIBE,
        Self::ETH_UNSUBSCRIBE,
        Self::ETH_SUBSCRIPTION,
    ];

    /// Useful for circumventing lifetimes associated with `let` bindings.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::BlockNumber => Self::ETH_BLOCK_NUMBER,
            Self::GetBlockByNumber => Self::ETH_GET_BLOCK_BY_NUMBER,
            Self::Syncing => Self::ETH_SYNCING,
            Self::GetBalance => Self::ETH_GET_BALANCE,
            Self::GetStorageAt => Self::ETH_GET_STORAGE_AT,
            Self::GetTransactionCount => Self::ETH_GET_TRANSACTION_COUNT,
            Self::GetBlockTransactionCountByNumber => {
                Self::ETH_GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER
            }
            Self::GetUncleCountByBlockNumber => Self::ETH_GET_UNCLE_COUNT_BY_BLOCK_NUMBER,
            Self::GetCode => Self::ETH_GET_CODE,
            Self::Call => Self::ETH_CALL,
            Self::GetTransactionByBlockNumberAndIndex => {
                Self::ETH_GET_TRANSACTION_BY_BLOCK_NUMBER_AND_INDEX
            }
            Self::GetUncleByBlockNumberAndIndex => Self::ETH_GET_UNCLE_BY_BLOCK_NUMBER_AND_INDEX,
            Self::Subscribe => Self::ETH_SUBSCRIBE,
            Self::Unsubscribe => Self::ETH_UNSUBSCRIBE,
            Self::Subscription => Self::ETH_SUBSCRIPTION,
        }
    }

    /// Determine the correct parameter index based on the method
    pub fn get_position<M: TryInto<Self>>(method: M) -> Option<usize> {
        match method.try_into() {
            Ok(Self::GetBalance)
            | Ok(Self::GetTransactionCount)
            | Ok(Self::GetCode)
            | Ok(Self::Call) => Some(1),
            Ok(Self::GetStorageAt) => Some(2),
            Ok(Self::GetBlockTransactionCountByNumber)
            | Ok(Self::GetUncleCountByBlockNumber)
            | Ok(Self::GetBlockByNumber)
            | Ok(Self::GetTransactionByBlockNumberAndIndex)
            | Ok(Self::GetUncleByBlockNumberAndIndex) => Some(0),
            _ => None,
        }
    }
}

impl<T> PartialEq<T> for EthRpcMethod
where
    T: PartialEq<str>,
{
    fn eq(&self, other: &T) -> bool {
        other.eq(self.as_str())
    }
}
impl PartialEq<EthRpcMethod> for serde_json::Value {
    fn eq(&self, other: &EthRpcMethod) -> bool {
        other.eq(self)
    }
}

impl TryFrom<Option<&str>> for EthRpcMethod {
    type Error = Error<Option<String>>;
    fn try_from(value: Option<&str>) -> Result<Self, Self::Error> {
        match value {
            Some(Self::ETH_BLOCK_NUMBER) => Ok(Self::BlockNumber),
            Some(Self::ETH_GET_BLOCK_BY_NUMBER) => Ok(Self::GetBlockByNumber),
            Some(Self::ETH_SYNCING) => Ok(Self::Syncing),
            Some(Self::ETH_GET_BALANCE) => Ok(Self::GetBalance),
            Some(Self::ETH_GET_STORAGE_AT) => Ok(Self::GetStorageAt),
            Some(Self::ETH_GET_TRANSACTION_COUNT) => Ok(Self::GetTransactionCount),
            Some(Self::ETH_GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER) => {
                Ok(Self::GetBlockTransactionCountByNumber)
            }
            Some(Self::ETH_GET_UNCLE_COUNT_BY_BLOCK_NUMBER) => Ok(Self::GetUncleCountByBlockNumber),
            Some(Self::ETH_GET_CODE) => Ok(Self::GetCode),
            Some(Self::ETH_CALL) => Ok(Self::Call),
            Some(Self::ETH_GET_TRANSACTION_BY_BLOCK_NUMBER_AND_INDEX) => {
                Ok(Self::GetTransactionByBlockNumberAndIndex)
            }
            Some(Self::ETH_GET_UNCLE_BY_BLOCK_NUMBER_AND_INDEX) => {
                Ok(Self::GetUncleByBlockNumberAndIndex)
            }
            Some(Self::ETH_SUBSCRIBE) => Ok(Self::Subscribe),
            Some(Self::ETH_UNSUBSCRIBE) => Ok(Self::Unsubscribe),
            Some(Self::ETH_SUBSCRIPTION) => Ok(Self::Subscription),
            _ => Err(Error::new(value.map(ToString::to_string))),
        }
    }
}

impl AsRef<str> for EthRpcMethod {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for EthRpcMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl serde::Serialize for EthRpcMethod {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> serde::Deserialize<'de> for EthRpcMethod {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        match s {
            Self::ETH_BLOCK_NUMBER => Ok(Self::BlockNumber),
            Self::ETH_GET_BLOCK_BY_NUMBER => Ok(Self::GetBlockByNumber),
            Self::ETH_SYNCING => Ok(Self::Syncing),
            Self::ETH_GET_BALANCE => Ok(Self::GetBalance),
            Self::ETH_GET_STORAGE_AT => Ok(Self::GetStorageAt),
            Self::ETH_GET_TRANSACTION_COUNT => Ok(Self::GetTransactionCount),
            Self::ETH_GET_BLOCK_TRANSACTION_COUNT_BY_NUMBER => {
                Ok(Self::GetBlockTransactionCountByNumber)
            }
            Self::ETH_GET_UNCLE_COUNT_BY_BLOCK_NUMBER => Ok(Self::GetUncleCountByBlockNumber),
            Self::ETH_GET_CODE => Ok(Self::GetCode),
            Self::ETH_CALL => Ok(Self::Call),
            Self::ETH_GET_TRANSACTION_BY_BLOCK_NUMBER_AND_INDEX => {
                Ok(Self::GetTransactionByBlockNumberAndIndex)
            }
            Self::ETH_GET_UNCLE_BY_BLOCK_NUMBER_AND_INDEX => {
                Ok(Self::GetUncleByBlockNumberAndIndex)
            }
            Self::ETH_SUBSCRIBE => Ok(Self::Subscribe),
            Self::ETH_UNSUBSCRIBE => Ok(Self::Unsubscribe),
            Self::ETH_SUBSCRIPTION => Ok(Self::Subscription),
            _ => Err(serde::de::Error::unknown_variant(s, Self::ETH_ALL)),
        }
    }
}
