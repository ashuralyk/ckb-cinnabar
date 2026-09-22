//! Minimal cell-lookup subset of [`RPC`](crate::kernel::rpc::RPC).
//!
//! These three methods match the first SSRI off-chain syscalls. They are a
//! **derivable subset** of [`Node`](crate::kernel::rpc::Node) +
//! [`Indexer`](crate::kernel::indexer::Indexer), not the ceiling of chain
//! access. Any `Node + Indexer` gets a [`Source`] impl automatically.
//!
//! [`UnsupportedSource`] stays for inject → operate → pack, which must not
//! call chain I/O.

use alloc::vec::Vec;

use crate::kernel::{
    error::{CalculatorError, Result},
    indexer::{Indexer, ScriptType, SearchKey, SearchMode},
    rpc::Node,
    types::packed,
};

/// Minimal cell lookup used by inject-path and type-id cell-dep operations.
pub trait Source {
    /// First live cell whose type script equals `type_script`.
    fn find_out_point_by_type(&self, type_script: &packed::Script) -> Result<packed::OutPoint>;

    /// Live cell output at `out_point` (no data).
    fn find_cell_by_out_point(&self, out_point: &packed::OutPoint) -> Result<packed::CellOutput>;

    /// Live cell data at `out_point`.
    fn find_cell_data_by_out_point(&self, out_point: &packed::OutPoint) -> Result<Vec<u8>>;
}

impl<T: Node + Indexer> Source for T {
    fn find_out_point_by_type(&self, type_script: &packed::Script) -> Result<packed::OutPoint> {
        let key = SearchKey {
            script: type_script.clone(),
            script_type: ScriptType::Type,
            script_search_mode: Some(SearchMode::Exact),
            filter: None,
            with_data: Some(false),
            group_by_transaction: None,
        };
        let page = self.get_cells(&key, 1, None)?;
        page.objects
            .into_iter()
            .next()
            .map(|cell| cell.out_point)
            .ok_or_else(|| {
                CalculatorError::CellDepNotFound("no live cell with this type script".into())
            })
    }

    fn find_cell_by_out_point(&self, out_point: &packed::OutPoint) -> Result<packed::CellOutput> {
        Ok(self.get_live_cell(out_point, false)?.output)
    }

    fn find_cell_data_by_out_point(&self, out_point: &packed::OutPoint) -> Result<Vec<u8>> {
        Ok(self.get_live_cell(out_point, true)?.output_data)
    }
}

/// Placeholder [`Source`] that always returns [`CalculatorError::SourceUnavailable`].
///
/// Use this for the inject → operate → pack path, which must not call chain I/O.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnsupportedSource;

impl Source for UnsupportedSource {
    fn find_out_point_by_type(&self, _type_script: &packed::Script) -> Result<packed::OutPoint> {
        Err(CalculatorError::SourceUnavailable(
            "Source is unset (inject-only assembly, or no RPC adapter)".into(),
        ))
    }

    fn find_cell_by_out_point(&self, _out_point: &packed::OutPoint) -> Result<packed::CellOutput> {
        Err(CalculatorError::SourceUnavailable(
            "Source is unset (inject-only assembly, or no RPC adapter)".into(),
        ))
    }

    fn find_cell_data_by_out_point(&self, _out_point: &packed::OutPoint) -> Result<Vec<u8>> {
        Err(CalculatorError::SourceUnavailable(
            "Source is unset (inject-only assembly, or no RPC adapter)".into(),
        ))
    }
}
