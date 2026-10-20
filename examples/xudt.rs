//! Build an xUDT mint and transfer without a node.
//!
//! `mint_xudt` / `transfer_xudt` only record operations. Sending them still
//! needs a live `RpcClient` (see `examples/dao.rs`). This example prints the
//! intent names so an agent can see the Calculate side of xUDT offline.
//!
//! ```text
//! cargo run --example xudt
//! ```

use ckb_cinnabar_calculator::{
    address::Address,
    instruction::predefined::{mint_xudt, transfer_xudt},
};

fn main() {
    let issuer: Address = "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqfqmf4hphl9jkrw3934mwe6m3a2nx88rzgdlw820"
        .parse()
        .expect("issuer");
    let holder: Address = "ckt1qzda0cr08m85hc8jlnfp3zer7xulejywt49kt2rr0vthywaa50xwsqtz32u8mgzk57zdtt6n62z4y2zyh8egkdcahyxk3"
        .parse()
        .expect("holder");
    let mint = mint_xudt(&issuer, &holder, 1_000, Vec::new());
    let transfer = transfer_xudt(&holder, &issuer, &issuer, 40, Vec::new());
    println!("mint instruction: {}", mint.name);
    println!("transfer instruction: {}", transfer.name);
    println!("issuer network: {}", issuer.network());
}
