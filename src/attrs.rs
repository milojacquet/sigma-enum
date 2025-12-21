use crate::nice_type::NiceTypeLit;
use std::collections::BTreeMap;
use syn::Expr;
use syn::ExprRange;
use syn::Ident;
use syn::Meta;
use syn::MetaList;
use syn::Token;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;

#[derive(Debug, Clone)]
struct GenericSpec(Ident, Vec<Option<Ident>>);

impl Parse for GenericSpec {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        let _: Token![<] = input.parse()?;
        let params = Punctuated::<_, Token![,]>::parse_separated_nonempty_with(
            input,
            |input: ParseStream| {
                // it can only be a single ident or _
                if input.parse::<Token![_]>().is_ok() {
                    Ok(None)
                } else {
                    input.parse::<Ident>().map(Some)
                }
            },
        )?
        .into_iter()
        .collect();
        let _: Token![>] = input.parse()?;
        Ok(Self(ident, params))
    }
}

#[derive(Debug, Clone, Default)]
pub struct ItemAttr {
    pub generics: BTreeMap<Ident, Vec<Option<Ident>>>,
}

impl Parse for ItemAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut out = Self::default();
        let metas: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;
        for meta in metas {
            match meta.path().require_ident()?.to_string().as_str() {
                "generic" => {
                    let Meta::List(MetaList { tokens, .. }) = meta else {
                        return Err(syn::Error::new(meta.span(), "not list"));
                    };
                    let generics: Punctuated<GenericSpec, Token![,]> =
                        Parser::parse2(Punctuated::parse_terminated, tokens)?;
                    for GenericSpec(ident, params) in generics {
                        if out.generics.insert(ident.clone(), params).is_some() {
                            return Err(syn::Error::new(ident.span(), "duplicate ident"));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

fn extract_expansion_range(expr: &ExprRange) -> syn::Result<Vec<NiceTypeLit>> {
    let start = match &expr.start {
        Some(expr) => match &**expr {
            Expr::Lit(expr) => match &expr.lit {
                syn::Lit::Int(lit_int) => lit_int,
                _ => return Err(syn::Error::new(expr.span(), "bad literal")),
            },
            _ => return Err(syn::Error::new(expr.span(), "not literal")),
        },
        None => return Err(syn::Error::new(expr.span(), "no start")),
    };
    let end = match &expr.end {
        Some(expr) => match &**expr {
            Expr::Lit(expr) => match &expr.lit {
                syn::Lit::Int(lit_int) => lit_int,
                _ => return Err(syn::Error::new(expr.span(), "bad literal")),
            },
            _ => return Err(syn::Error::new(expr.span(), "not literal")),
        },
        None => return Err(syn::Error::new(expr.span(), "no end")),
    };

    if let Ok(start) = start.base10_parse::<i128>()
        && start < 1 << 126
    {
        let end: i128 = end.base10_parse()?;
        if end - start > 65536 {
            return Err(syn::Error::new(
                expr.span(),
                format!("range too large ({} > 65536)", end - start),
            ));
        }
        Ok(match expr.limits {
            syn::RangeLimits::HalfOpen(_) => (start..end)
                .map(|n| NiceTypeLit::Int(n.to_string()))
                .collect(),
            syn::RangeLimits::Closed(_) => (start..=end)
                .map(|n| NiceTypeLit::Int(n.to_string()))
                .collect(),
        })
    } else {
        let start: u128 = start.base10_parse()?;
        let end: u128 = end.base10_parse()?;
        if end - start > 65536 {
            return Err(syn::Error::new(
                expr.span(),
                format!("range too large ({} > 65536)", end - start),
            ));
        }
        Ok(match expr.limits {
            syn::RangeLimits::HalfOpen(_) => (start..end)
                .map(|n| NiceTypeLit::Int(n.to_string()))
                .collect(),
            syn::RangeLimits::Closed(_) => (start..=end)
                .map(|n| NiceTypeLit::Int(n.to_string()))
                .collect(),
        })
    }
}

pub fn extract_expansion(expr: &Expr) -> syn::Result<Vec<NiceTypeLit>> {
    match expr {
        Expr::Lit(expr) => Ok(vec![NiceTypeLit::from_lit(&expr.lit)?]),
        Expr::Range(expr) => extract_expansion_range(expr),
        Expr::Array(expr) => {
            let mut out = Vec::new();
            for expr in &expr.elems {
                match expr {
                    Expr::Lit(expr) => out.push(NiceTypeLit::from_lit(&expr.lit)?),
                    Expr::Range(expr) => out.extend(extract_expansion_range(&expr)?),
                    _ => return Err(syn::Error::new(expr.span(), "invalid value")),
                }
            }
            Ok(out)
        }
        _ => Err(syn::Error::new(expr.span(), "invalid value")),
    }
}
