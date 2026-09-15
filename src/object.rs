//! CLI value types and the JSON envelope printed by `--json`.
//!
//! [`DeploymentRecord`] is the on-disk history of one contract. [`CliResponse`]
//! is the stdout object agents parse: `ok`, `error.kind`, `error.exit_code`.

use std::{fmt::Display, str::FromStr};

use ckb_cinnabar_calculator::{
    address::Address,
    error::CalculatorError,
    re_exports::{
        ckb_types::{core, packed, prelude::*, H256},
        eyre,
    },
    rpc::Network,
    skeleton::ScriptEx,
};
use serde::{Deserialize, Serialize};

/// How `migrate` handles the contract cell's type id.
#[derive(PartialEq, Eq, Clone)]
pub enum TypeIdMode {
    /// Keep the existing type id on the new contract cell.
    Keep,
    /// Drop the type id (cell becomes non-upgradable).
    Remove,
    /// Melt the old type id and mint a fresh one.
    New,
}

impl FromStr for TypeIdMode {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "keep" => Ok(TypeIdMode::Keep),
            "remove" => Ok(TypeIdMode::Remove),
            "new" => Ok(TypeIdMode::New),
            _ => Err(eyre::eyre!("invalid type_id_mode")),
        }
    }
}

impl Display for TypeIdMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self {
            TypeIdMode::Keep => "keep",
            TypeIdMode::Remove => "remove",
            TypeIdMode::New => "new",
        };
        write!(f, "{inner}",)
    }
}

/// Which records `list` prints.
#[derive(PartialEq, Eq, Clone)]
pub enum ListMode {
    /// Everything.
    All,
    /// Only live deployments.
    Deployed,
    /// Only consumed (destroyed) contract cells.
    Consumed,
}

impl TryFrom<String> for ListMode {
    type Error = eyre::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for ListMode {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all" => Ok(ListMode::All),
            "deployed" => Ok(ListMode::Deployed),
            "consumed" => Ok(ListMode::Consumed),
            _ => Err(eyre::eyre!("invalid list mode")),
        }
    }
}

impl Display for ListMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self {
            ListMode::All => "all",
            ListMode::Deployed => "deployed",
            ListMode::Consumed => "consumed",
        };
        write!(f, "{inner}")
    }
}

/// CLI/JSON-friendly optional CKB address. Serializes as a string (`""` when
/// absent) so deployment records stay hand-editable.
#[derive(Clone, Default)]
pub struct CkbAddress(Option<Address>);

impl TryFrom<CkbAddress> for Address {
    type Error = eyre::Error;

    fn try_from(value: CkbAddress) -> Result<Self, Self::Error> {
        value.0.ok_or_else(|| eyre::eyre!("empty ckb address"))
    }
}

impl From<Address> for CkbAddress {
    fn from(value: Address) -> Self {
        CkbAddress(Some(value))
    }
}

impl From<Option<Address>> for CkbAddress {
    fn from(value: Option<Address>) -> Self {
        CkbAddress(value)
    }
}

impl FromStr for CkbAddress {
    type Err = eyre::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Address::from_str(value)
            .map(|v| CkbAddress(Some(v)))
            .map_err(|_| eyre::eyre!("invalid ckb address"))
    }
}

impl Display for CkbAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.0.as_ref().map(|v| v.to_string()).unwrap_or_default();
        write!(f, "{inner}",)
    }
}

impl<'de> Deserialize<'de> for CkbAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            Ok(CkbAddress(None))
        } else {
            value.parse().map_err(serde::de::Error::custom)
        }
    }
}

impl Serialize for CkbAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

/// One line of deployment history, persisted as
/// `deployment/<network>/<contract>.json`.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct DeploymentRecord {
    /// Contract name (`--contract-name`).
    pub name: String,
    /// Operation timestamp string.
    pub date: String,
    /// `deploy` / `migrate` / `consume`.
    pub operation: String,
    /// Version tag (`--tag` / `--to-tag`).
    pub version: String,
    /// Hash of the transaction that created/consumed the contract cell.
    pub tx_hash: H256,
    /// Index of the contract cell inside `tx_hash` outputs.
    pub out_index: u32,
    /// Data hash of the contract binary; `None` once the cell is consumed.
    pub data_hash: Option<H256>,
    /// Occupied capacity of the contract cell in shannons.
    pub occupied_capacity: u64,
    /// Who paid the capacity and fee.
    pub payer_address: CkbAddress,
    /// Lock owner of the contract cell.
    pub contract_owner_address: CkbAddress,
    /// Type id of the contract cell when deployed/migrated with one.
    pub type_id: Option<H256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Alias of `type_id` kept for record readability.
    pub type_id_args: Option<H256>,
    // This field is not required, so you can edit in your <contract>.json file to add comment for cooperations
    #[serde(default, rename = "__comment")]
    /// Free-form note stored inside the record file.
    pub comment: Option<String>,
}

impl DeploymentRecord {
    /// Build the script that references this deployment: by type id when the
    /// contract was deployed with one, otherwise by `Data2` data hash.
    pub fn generate_script(&self, args: Vec<u8>) -> eyre::Result<ScriptEx> {
        let mut script = packed::Script::new_builder().args(args.pack());
        if let Some(type_id) = self.type_id.clone() {
            script = script
                .code_hash(type_id.0.pack())
                .hash_type(core::ScriptHashType::Type);
        } else {
            let Some(data_hash) = self.data_hash.clone() else {
                return Err(eyre::eyre!("contract consumed"));
            };
            script = script
                .code_hash(data_hash.0.pack())
                .hash_type(core::ScriptHashType::Data2);
        }
        Ok(script.build().into())
    }
}

/// Shared CLI execution flags.
#[derive(Clone)]
pub struct ExecOpts {
    /// Target network (`mainnet` / `testnet` / custom URL).
    pub network: Network,
    /// Directory holding deployment record JSON files.
    pub deployment_path: String,
    /// Directory holding compiled RISC-V contract binaries.
    pub contract_path: String,
    /// Emit the machine-readable [`CliResponse`] JSON envelope.
    pub json: bool,
    /// Assemble (and sign) but never send.
    pub dry_run: bool,
    /// Headless signing key from `--privkey-env`; `None` falls back to ckb-cli.
    pub privkey: Option<ckb_cinnabar_calculator::re_exports::secp256k1::SecretKey>,
}

/// Machine-readable CLI result (`--json`).
#[derive(Serialize)]
pub struct CliResponse {
    /// Whether the operation succeeded.
    pub ok: bool,
    /// Subcommand name (`deploy` / `migrate` / `consume` / `list`).
    pub operation: String,
    /// Whether the transaction was only assembled, not sent.
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Hash of the sent transaction (absent on dry runs and failures).
    pub transaction_hash: Option<H256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The record written by deploy/migrate/consume.
    pub record: Option<DeploymentRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Records printed by `list`.
    pub records: Option<Vec<DeploymentRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Structured failure; present iff `ok` is false.
    pub error: Option<CliErrorBody>,
}

/// `error` payload of a failed [`CliResponse`].
#[derive(Serialize)]
pub struct CliErrorBody {
    /// Stable machine-readable category, from [`CalculatorError::kind`] or
    /// [`CliError::kind`].
    pub kind: String,
    /// Human-readable detail.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// On-chain script exit code for contract validation failures.
    pub exit_code: Option<i8>,
}

/// CLI-level failure that is not a calculator error (bad flags, env, files).
#[derive(Debug)]
pub enum CliError {
    /// Missing/invalid environment or configuration.
    Configuration(String),
    /// Malformed CLI argument value.
    InvalidInput(String),
}

impl CliError {
    /// Stable kind string (`configuration` / `invalid_input`) for JSON.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Configuration(_) => "configuration",
            Self::InvalidInput(_) => "invalid_input",
        }
    }

    /// Human-readable detail without the kind prefix.
    pub fn message(&self) -> &str {
        match self {
            Self::Configuration(message) | Self::InvalidInput(message) => message,
        }
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for CliError {}

impl CliErrorBody {
    /// Build a JSON error body from an eyre report, preferring the typed
    /// [`CliError`] / [`CalculatorError`] kinds when present.
    pub fn from_report(error: &eyre::Report) -> Self {
        if let Some(error) = error.downcast_ref::<CliError>() {
            return Self {
                kind: error.kind().to_string(),
                message: error.message().to_string(),
                exit_code: None,
            };
        }
        if let Some(error) = error.downcast_ref::<CalculatorError>() {
            return Self {
                kind: error.kind().to_string(),
                message: error.message().to_string(),
                exit_code: error.script_exit_code(),
            };
        }
        Self {
            kind: "other".to_string(),
            message: format!("{error:#}"),
            exit_code: None,
        }
    }
}

impl CliResponse {
    /// Successful result envelope for `operation`.
    pub fn ok(operation: &str, dry_run: bool) -> Self {
        Self {
            ok: true,
            operation: operation.to_string(),
            dry_run,
            transaction_hash: None,
            record: None,
            records: None,
            error: None,
        }
    }

    /// Failed result envelope for `operation`, classifying `error` into a
    /// stable [`CliErrorBody`].
    pub fn error(operation: &str, dry_run: bool, error: &eyre::Report) -> Self {
        Self {
            ok: false,
            operation: operation.to_string(),
            dry_run,
            transaction_hash: None,
            record: None,
            records: None,
            error: Some(CliErrorBody::from_report(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_error_preserves_script_exit_code() {
        let report = eyre::Report::new(CalculatorError::ScriptValidation {
            exit_code: 20,
            message: "contract rejected transaction".to_string(),
        });

        let error = CliErrorBody::from_report(&report);

        assert_eq!(error.kind, "script_validation");
        assert_eq!(error.exit_code, Some(20));
    }
}
