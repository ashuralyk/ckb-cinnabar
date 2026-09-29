//! Re-quote SSRI method names for `ssri_methods!`.
//!
//! `cinnabar_main!` captures each wire name as a `macro_rules` literal. Pasting
//! that capture into `ssri_methods!` wraps the literal in an invisible group,
//! and `ssri_methods!` only accepts a bare string literal (`Expr::Lit`). This
//! macro reads either form and emits a fresh string literal.

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, Expr, ExprLit, Ident, Lit, LitStr, Token,
};

struct WireTable {
    argv: Expr,
    invalid_method: Expr,
    invalid_args: Expr,
    methods: Vec<Wire>,
}

struct Wire {
    name: LitStr,
    body: Expr,
}

impl Parse for WireTable {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _argv_label: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let argv = input.parse()?;
        input.parse::<Token![,]>()?;
        let _invalid_method_label: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let invalid_method = input.parse()?;
        input.parse::<Token![,]>()?;
        let _invalid_args_label: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let invalid_args = input.parse()?;
        input.parse::<Token![,]>()?;

        let mut methods = Vec::new();
        while !input.is_empty() {
            let name = method_name(input.parse()?)?;
            input.parse::<Token![=>]>()?;
            let body = peel_groups(input.parse()?);
            input.parse::<Token![,]>()?;
            methods.push(Wire { name, body });
        }

        Ok(WireTable {
            argv,
            invalid_method,
            invalid_args,
            methods,
        })
    }
}

impl WireTable {
    fn emit(&self) -> proc_macro2::TokenStream {
        let argv = &self.argv;
        let invalid_method = &self.invalid_method;
        let invalid_args = &self.invalid_args;
        let names = self.methods.iter().map(|method| &method.name);
        let bodies = self.methods.iter().map(|method| &method.body);
        quote! {
            ckb_cinnabar_verifier::ssri_methods!(
                argv: #argv,
                invalid_method: #invalid_method,
                invalid_args: #invalid_args,
                #(#names => #bodies,)*
            )
        }
    }
}

fn peel_groups(mut expr: Expr) -> Expr {
    while let Expr::Group(group) = expr {
        expr = *group.expr;
    }
    expr
}

fn method_name(expr: Expr) -> syn::Result<LitStr> {
    let expr = peel_groups(expr);
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(lit), ..
        }) => Ok(LitStr::new(&lit.value(), lit.span())),
        other => Err(syn::Error::new_spanned(
            other,
            "SSRI method name must be a string literal",
        )),
    }
}

/// Rebuild a `ssri_methods!` invocation whose method names are bare string literals.
#[proc_macro]
pub fn expand_ssri_methods(input: TokenStream) -> TokenStream {
    let table = parse_macro_input!(input as WireTable);
    table.emit().into()
}

#[cfg(test)]
mod tests {
    use proc_macro2::{Delimiter, Group, Spacing, TokenStream, TokenTree};
    use quote::quote;
    use syn::parse2;

    use super::WireTable;

    fn none(tokens: TokenStream) -> TokenStream {
        TokenStream::from(TokenTree::Group(Group::new(Delimiter::None, tokens)))
    }

    fn expand(input: TokenStream) -> syn::Result<TokenStream> {
        let table: WireTable = parse2(input)?;
        Ok(table.emit())
    }

    fn names_before_fat_arrow(stream: TokenStream) -> Vec<String> {
        let mut names = Vec::new();
        let mut pending = Vec::new();
        for tree in stream {
            match tree {
                TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => {
                    names.extend(names_before_fat_arrow(group.stream()));
                }
                TokenTree::Group(group) => pending.push(TokenTree::Group(group)),
                other => pending.push(other),
            }
        }
        for window in pending.windows(3) {
            let is_arrow = matches!(
                (&window[1], &window[2]),
                (TokenTree::Punct(eq), TokenTree::Punct(gt))
                    if eq.as_char() == '='
                        && eq.spacing() == Spacing::Joint
                        && gt.as_char() == '>'
            );
            if is_arrow {
                match &window[0] {
                    TokenTree::Literal(lit) => names.push(lit.to_string()),
                    other => panic!("method name was wrapped: {other:?}"),
                }
            }
        }
        names
    }

    #[test]
    fn grouped_macro_rules_literal_is_requoted() {
        let captured = none(none(quote!("UDT.mint")));
        let input = quote! {
            argv: &argv,
            invalid_method: Missing,
            invalid_args: Bad,
            #captured => export(&argv, mint),
            "UDT.decimals" => export(&argv, 8u8),
        };
        let names = names_before_fat_arrow(expand(input).unwrap());
        assert_eq!(names, ["\"UDT.mint\"", "\"UDT.decimals\""]);
    }

    #[test]
    fn non_string_name_is_rejected() {
        let err = expand(quote! {
            argv: &argv,
            invalid_method: Missing,
            invalid_args: Bad,
            7 => body,
        })
        .unwrap_err();
        assert!(err.to_string().contains("string literal"));
    }
}
