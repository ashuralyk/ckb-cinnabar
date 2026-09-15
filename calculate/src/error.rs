//! Typed errors for off-chain assembly. Operation implementations may still
//! use `eyre`; [`Instruction`](crate::instruction::Instruction) and
//! [`TransactionCalculator`](crate::instruction::TransactionCalculator) surface
//! [`CalculatorError`] so agents can branch without scraping strings.

use std::fmt;

/// Calculator result type used by public assembly / simulation APIs.
pub type Result<T> = std::result::Result<T, CalculatorError>;

/// Recoverable off-chain assembly / simulation / RPC failure.
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
    ScriptValidation { exit_code: i8, message: String },
    /// Misuse of the offline [`crate::simulation::FakeRpcClient`].
    FakeRpc(String),
    /// Filesystem failure (contract binary, deployment record, ...).
    Io(String),
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

    /// Classify a free-form `eyre` report into a typed variant.
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
        if lower.contains("cell dep not found")
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
        } else if lower.contains("sign") || lower.contains("ckb-cli") || lower.contains("password")
        {
            Self::Signing(msg.to_string())
        } else if lower.contains("fake") && lower.contains("rpc") {
            Self::FakeRpc(msg.to_string())
        } else if lower.contains("simulation")
            || (lower.contains("script") && lower.contains("error"))
        {
            Self::Simulation(msg.to_string())
        } else {
            Self::Other(msg.to_string())
        }
    }
}

impl fmt::Display for CalculatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for CalculatorError {}

impl From<eyre::Report> for CalculatorError {
    fn from(err: eyre::Report) -> Self {
        Self::from_message(format!("{err:#}"))
    }
}

impl From<std::io::Error> for CalculatorError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl serde::Serialize for CalculatorError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
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
}
