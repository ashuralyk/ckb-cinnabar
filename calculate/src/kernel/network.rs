//! Network selector used by hardcoded script hashes.
//!
//! `Custom(url)` exists only on the host (`std`) profile.

/// The CKB network an assembler is targeting.
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub enum Network {
    /// CKB mainnet (address prefix `ckb`).
    Mainnet,
    /// CKB testnet (address prefix `ckt`).
    Testnet,
    /// A self-hosted or dev chain reachable at a custom RPC URL.
    #[cfg(feature = "std")]
    Custom(reqwest::Url),
    /// Offline / injected-cell network (FakeRpc or SSRI inject path).
    Fake,
}

impl Network {
    /// Resolve a network from a bech32(m) address HRP: `ckb` → mainnet,
    /// `ckt` → testnet, anything else → `None`.
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "ckb" => Some(Network::Mainnet),
            "ckt" => Some(Network::Testnet),
            _ => None,
        }
    }

    /// Bech32(m) HRP for this network. Everything except mainnet uses `ckt`.
    pub fn to_prefix(&self) -> &'static str {
        match self {
            Network::Mainnet => "ckb",
            _ => "ckt",
        }
    }
}

use core::fmt;

#[cfg(feature = "std")]
use std::str::FromStr;

#[cfg(feature = "std")]
impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Testnet => write!(f, "testnet"),
            Network::Fake => write!(f, "fake"),
            Network::Custom(url) => write!(f, "{}", url),
        }
    }
}

#[cfg(feature = "std")]
impl FromStr for Network {
    type Err = eyre::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "fake" => Ok(Network::Fake),
            _ => Ok(Network::Custom(value.parse()?)),
        }
    }
}

#[cfg(not(feature = "std"))]
impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Network::Mainnet => f.write_str("mainnet"),
            Network::Testnet => f.write_str("testnet"),
            Network::Fake => f.write_str("fake"),
        }
    }
}
