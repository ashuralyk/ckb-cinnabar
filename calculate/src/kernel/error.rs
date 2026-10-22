//! Typed errors for assembly. Host operations may still use `eyre`;
//! [`Instruction`](crate::instruction::Instruction) surfaces [`CalculatorError`].

use alloc::string::{String, ToString};
use core::{fmt, result};

/// Calculator result type used by public assembly / simulation APIs.
pub type Result<T> = result::Result<T, CalculatorError>;

/// Recoverable assembly / simulation / RPC / source failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalculatorError {
    /// A referenced cell dep could not be located on-chain.
    CellDepNotFound(String),
    /// No live input cell matched the query.
    InputCellNotFound(String),
    /// A referenced output cell index/position is missing.
    OutputCellNotFound(String),
    /// Inputs cannot cover outputs + fee.
    InsufficientCapacity(String),
    /// Indexer returned no usable cells for a required query.
    NoAvailableCells(String),
    /// RPC / transport failure.
    Network(String),
    /// Signing failed (bad key, ckb-cli error, ...).
    Signing(String),
    /// Transaction skeleton or CKB-VM level failure without a script exit code.
    Simulation(String),
    /// On-chain script rejected the transaction; `exit_code` is the script's
    /// `i8` (see `define_errors!` on the Verify side).
    ScriptValidation {
        /// Script `i8` exit code (`define_errors!` on the Verify side).
        exit_code: i8,
        /// Human-readable VM or script message.
        message: String,
    },
    /// Misuse of the offline [`crate::simulation::FakeRpcClient`].
    FakeRpc(String),
    /// Filesystem or other `std::io` failure (contract binary, deployment record, ...).
    Io(String),
    /// [`crate::source::Source`] is unset or the lookup is not implemented.
    SourceUnavailable(String),
    /// Anything that does not fit the variants above.
    Other(String),
}

impl CalculatorError {
    /// Stable error kind for JSON / agent branching.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CellDepNotFound(_) => "cell_dep_not_found",
            Self::InputCellNotFound(_) => "input_cell_not_found",
            Self::OutputCellNotFound(_) => "output_cell_not_found",
            Self::InsufficientCapacity(_) => "insufficient_capacity",
            Self::NoAvailableCells(_) => "no_available_cells",
            Self::Network(_) => "network",
            Self::Signing(_) => "signing",
            Self::Simulation(_) => "simulation",
            Self::ScriptValidation { .. } => "script_validation",
            Self::FakeRpc(_) => "fake_rpc",
            Self::Io(_) => "io",
            Self::SourceUnavailable(_) => "source_unavailable",
            Self::Other(_) => "other",
        }
    }

    /// Human-readable detail message (without the `kind` prefix).
    pub fn message(&self) -> &str {
        match self {
            Self::CellDepNotFound(m)
            | Self::InputCellNotFound(m)
            | Self::OutputCellNotFound(m)
            | Self::InsufficientCapacity(m)
            | Self::NoAvailableCells(m)
            | Self::Network(m)
            | Self::Signing(m)
            | Self::Simulation(m)
            | Self::FakeRpc(m)
            | Self::Io(m)
            | Self::SourceUnavailable(m)
            | Self::Other(m) => m,
            Self::ScriptValidation { message, .. } => message,
        }
    }

    /// Structured CKB-VM exit code, when this is a script validation failure.
    pub fn script_exit_code(&self) -> Option<i8> {
        match self {
            Self::ScriptValidation { exit_code, .. } => Some(*exit_code),
            _ => None,
        }
    }

    /// Classify a free-form report into a typed variant.
    pub fn from_message(msg: impl AsRef<str>) -> Self {
        let msg = msg.as_ref();
        let lower = msg.to_ascii_lowercase();
        if lower.contains("validationfailure") {
            if let Some(exit_code) = script_exit_code_from_str(msg) {
                return Self::ScriptValidation {
                    exit_code,
                    message: msg.to_string(),
                };
            }
        }
        if lower.contains("source") && lower.contains("unavailable") {
            Self::SourceUnavailable(msg.to_string())
        } else if lower.contains("cell dep not found")
            || (lower.contains("celldep") && lower.contains("not found"))
        {
            Self::CellDepNotFound(msg.to_string())
        } else if lower.contains("input cell not found")
            || lower.contains("transaction input empty")
            || lower.contains("no available input")
        {
            Self::InputCellNotFound(msg.to_string())
        } else if lower.contains("output index") || lower.contains("no output") {
            Self::OutputCellNotFound(msg.to_string())
        } else if lower.contains("capacity")
            && (lower.contains("less than")
                || lower.contains("not enough")
                || lower.contains("cannot cover")
                || lower.contains("insufficient"))
        {
            Self::InsufficientCapacity(msg.to_string())
        } else if lower.contains("no available") {
            Self::NoAvailableCells(msg.to_string())
        } else if !is_filesystem_message(&lower) && is_signing_message(&lower) {
            Self::Signing(msg.to_string())
        } else if lower.contains("fake") && lower.contains("rpc") {
            Self::FakeRpc(msg.to_string())
        } else if !is_filesystem_message(&lower)
            && (lower.contains("simulation")
                || (lower.contains("script") && lower.contains("error")))
        {
            Self::Simulation(msg.to_string())
        } else if lower.contains("bad ckb request")
            || lower.contains("error sending request")
            || lower.contains("connection refused")
            || lower.contains("failed to parse json response")
            || lower.contains("failed to get response from ckb rpc")
        {
            Self::Network(msg.to_string())
        } else if is_filesystem_message(&lower) {
            Self::Io(msg.to_string())
        } else {
            Self::Other(msg.to_string())
        }
    }

    /// Classify an [`eyre::Report`] the same way the CLI JSON fallback does.
    ///
    /// [`Self::from_message`] runs on the full chain text, so transport phrases
    /// stay `network` even when a cause contains `os error`. A result that is
    /// still [`Self::Other`] becomes `io` when the chain holds a
    /// [`std::io::Error`] whose display text is not itself a filesystem
    /// marker (for example `contract file`, or
    /// `Cannot start a runtime from within a runtime`).
    #[cfg(feature = "std")]
    pub fn from_report(err: &eyre::Report) -> Self {
        let text = format!("{err:#}");
        let classified = Self::from_message(&text);
        if matches!(classified, Self::Other(_)) && report_chain_has_io(err) {
            Self::Io(text)
        } else {
            classified
        }
    }
}

/// `sign` and `signed` match as whole words. `signing`, `signature`,
/// `unsigned`, `ckb-cli`, and `password` match as substrings. `design` and
/// `assignment` do not.
fn is_signing_message(lower: &str) -> bool {
    contains_ascii_word(lower, "sign")
        || contains_ascii_word(lower, "signed")
        || lower.contains("signing")
        || lower.contains("signature")
        || lower.contains("unsigned")
        || lower.contains("ckb-cli")
        || lower.contains("password")
}

fn contains_ascii_word(haystack: &str, word: &str) -> bool {
    let hay = haystack.as_bytes();
    let needle = word.as_bytes();
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    hay.windows(needle.len())
        .enumerate()
        .any(|(index, window)| {
            if window != needle {
                return false;
            }
            let boundary_before = index == 0 || !hay[index - 1].is_ascii_alphanumeric();
            let end = index + needle.len();
            let boundary_after = end == hay.len() || !hay[end].is_ascii_alphanumeric();
            boundary_before && boundary_after
        })
}

/// Filesystem and missing-binary markers.
///
/// These win over the signing-word / `ckb-cli` / `script error` heuristics.
/// They do not win over transport phrases (`connection refused`,
/// `error sending request`), which are tested first.
fn is_filesystem_message(lower: &str) -> bool {
    lower.contains("os error")
        || lower.contains("no such file")
        || lower.contains("permission denied")
        || lower.contains("not a directory")
        || lower.contains("is a directory")
        || lower.contains("entity not found")
        || lower.contains("entity already exists")
        || lower.contains("directory not empty")
        || lower.contains("read-only file")
        || lower.contains("no space left on device")
        || lower.contains("no storage space")
        || lower.contains("missing binary")
        || (lower.contains("binary") && lower.contains("not found"))
}

#[cfg(feature = "std")]
fn report_chain_has_io(err: &eyre::Report) -> bool {
    err.chain()
        .any(|cause| cause.downcast_ref::<std::io::Error>().is_some())
}

impl fmt::Display for CalculatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CalculatorError {}

#[cfg(feature = "std")]
impl From<eyre::Report> for CalculatorError {
    fn from(err: eyre::Report) -> Self {
        Self::from_report(&err)
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for CalculatorError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

#[cfg(feature = "std")]
impl serde::Serialize for CalculatorError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("CalculatorError", 3)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", self.message())?;
        if let Some(exit_code) = self.script_exit_code() {
            s.serialize_field("exit_code", &exit_code)?;
        }
        s.end()
    }
}

/// Extract an on-chain `i8` exit code from a CKB-VM / ckb-script error string.
///
/// Best-effort parser used when errors arrive as plain text (e.g. via eyre);
/// prefer [`CalculatorError::script_exit_code`] when a typed error is available.
#[cfg(feature = "std")]
pub fn script_exit_code(err: &eyre::Report) -> Option<i8> {
    script_exit_code_from_str(&format!("{err:#}"))
}

/// Same as [`script_exit_code`] but for a raw message string. Recognizes
/// markers like `exit_code:`, `exit code `, `error code ` and
/// `ValidationFailure(`.
pub fn script_exit_code_from_str(msg: &str) -> Option<i8> {
    const MARKERS: [&str; 6] = [
        "exit_code:",
        "exit code ",
        "error code ",
        "code: ",
        "code:",
        "ValidationFailure(",
    ];
    for marker in MARKERS {
        if let Some(idx) = msg.find(marker) {
            let rest = msg[idx + marker.len()..].trim_start();
            let digits: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .collect();
            if let Ok(v) = digits.parse::<i8>() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_cell_dep() {
        let err = CalculatorError::from_message("cell dep not found");
        assert_eq!(err.kind(), "cell_dep_not_found");
    }

    #[test]
    fn parses_validation_failure() {
        assert_eq!(
            script_exit_code_from_str("ScriptError: ValidationFailure(20)"),
            Some(20)
        );
        assert_eq!(script_exit_code_from_str("exit code 11"), Some(11));
    }

    #[test]
    fn classifies_rpc_validation_failure_with_exit_code() {
        let error = CalculatorError::from_message("TransactionScriptError: ValidationFailure(20)");

        assert_eq!(error.kind(), "script_validation");
        assert_eq!(error.script_exit_code(), Some(20));

        let displayed = CalculatorError::from_message(
            "ValidationFailure: see error code -7 on page https://example.invalid",
        );
        assert_eq!(displayed.script_exit_code(), Some(-7));
    }

    #[test]
    fn classifies_transport_and_missing_file() {
        assert_eq!(
            CalculatorError::from_message(
                "bad ckb request url: error sending request for url (http://127.0.0.1:9/)"
            )
            .kind(),
            "network"
        );
        assert_eq!(
            CalculatorError::from_message(
                "No such file or directory (os error 2):build/release/my_lock"
            )
            .kind(),
            "io"
        );
    }

    #[test]
    fn filesystem_markers_are_io_not_signing_or_simulation() {
        assert_eq!(
            CalculatorError::from_message("ckb-cli: No such file or directory (os error 2)").kind(),
            "io"
        );
        assert_eq!(
            CalculatorError::from_message("script error: entity not found").kind(),
            "io"
        );
        assert_eq!(
            CalculatorError::from_message("is a directory (os error 21)").kind(),
            "io"
        );
        assert_eq!(
            CalculatorError::from_message("missing binary: build/release/my_lock").kind(),
            "io"
        );
        assert_eq!(
            CalculatorError::from_message("contract binary not found").kind(),
            "io"
        );
        assert_eq!(
            CalculatorError::from_message("failed to sign: bad password").kind(),
            "signing"
        );
        assert_eq!(
            CalculatorError::from_message("script design error").kind(),
            "simulation"
        );
        assert_eq!(
            CalculatorError::from_message("invalid assignment of output").kind(),
            "other"
        );
        assert_eq!(CalculatorError::from_message("signal lost").kind(), "other");
        assert_eq!(
            CalculatorError::from_message("connection refused (os error 111)").kind(),
            "network"
        );
        assert_eq!(
            CalculatorError::from_message(
                "bad ckb request url: error sending request for url (os error 111)"
            )
            .kind(),
            "network"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn eyre_io_without_os_error_text_is_io() {
        let missing = eyre::Report::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "contract file",
        ));
        assert_eq!(CalculatorError::from(missing).kind(), "io");

        let runtime = eyre::Report::from(std::io::Error::other(
            "Cannot start a runtime from within a runtime",
        ));
        assert_eq!(CalculatorError::from(runtime).kind(), "io");

        let transport = eyre::eyre!(
            "bad ckb request url: error sending request for url (http://127.0.0.1:9/) (os error 111)"
        );
        assert_eq!(CalculatorError::from(transport).kind(), "network");
    }
}
