use crate::nice_type::NiceTypeLit;
use quote::quote;
use std::collections::BTreeMap;
use syn::Expr;
use syn::ExprLit;
use syn::ExprPath;
use syn::ExprRange;
use syn::Ident;
use syn::Lit;
use syn::Meta;
use syn::MetaList;
use syn::MetaNameValue;
use syn::Path;
use syn::Token;
use syn::TypePath;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse::Parser;
use syn::parse2;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;

#[derive(Clone)]
struct GenericSpec(Ident, Vec<Option<TypePath>>);

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
                    input.parse::<TypePath>().map(Some)
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
pub struct PublicItem {
    pub name: Option<Ident>,
    pub docs: Option<String>,
}

impl PublicItem {
    pub fn docstring(&self) -> proc_macro2::TokenStream {
        self.docs
            .as_ref()
            .map_or_else(|| quote! {}, |docs| quote! { #[doc = #docs] })
    }
}

impl Parse for PublicItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut public_item = Self::default();
        let meta: Punctuated<MetaNameValue, Token![,]> = Punctuated::parse_terminated(input)?;
        for MetaNameValue { path, value, .. } in meta {
            match path.require_ident()?.to_string().as_str() {
                "name" => {
                    let Expr::Path(name) = value else {
                        return Err(syn::Error::new(value.span(), "not path"));
                    };
                    let name = name.path.require_ident()?;
                    public_item.name = Some(name.clone());
                }
                "docs" => {
                    let Expr::Lit(ExprLit {
                        lit: Lit::Str(docs),
                        ..
                    }) = value
                    else {
                        return Err(syn::Error::new(value.span(), "not string"));
                    };
                    public_item.docs = Some(docs.value());
                }
                _ => {}
            }
        }
        Ok(public_item)
    }
}

#[derive(Clone, Default)]
pub struct ItemAttr {
    pub generics: BTreeMap<Ident, Vec<Option<TypePath>>>,
    pub alias: BTreeMap<Ident, TypePath>,
    pub path: Option<Path>,

    pub macro_match: PublicItem,
    pub macro_construct: PublicItem,
    pub into_trait: PublicItem,
    pub into_method: PublicItem,
    pub try_from_method: PublicItem,
    pub try_from_owned_method: PublicItem,
    pub try_from_mut_method: PublicItem,
    pub extract_method: PublicItem,
    pub extract_owned_method: PublicItem,
    pub extract_mut_method: PublicItem,
    pub try_from_error: PublicItem,
}

impl Parse for ItemAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut attr = Self::default();
        let metas: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(input)?;
        for meta in metas {
            match meta.path().require_ident()?.to_string().as_str() {
                "generic" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let generics: Punctuated<GenericSpec, Token![,]> =
                        Parser::parse2(Punctuated::parse_terminated, tokens.clone())?;
                    for GenericSpec(ident, params) in generics {
                        if attr.generics.insert(ident.clone(), params).is_some() {
                            return Err(syn::Error::new(ident.span(), "duplicate ident"));
                        }
                    }
                }
                "alias" => {
                    let MetaList { tokens, .. } = meta.require_list()?;

                    let aliases: Punctuated<MetaNameValue, Token![,]> =
                        Parser::parse2(Punctuated::parse_terminated, tokens.clone())?;

                    for MetaNameValue {
                        path: ident, value, ..
                    } in aliases
                    {
                        let ident = ident.require_ident()?;
                        let Expr::Path(ExprPath { qself, path, .. }) = value else {
                            return Err(syn::Error::new(value.span(), "not path"));
                        };
                        attr.alias.insert(
                            ident.clone(),
                            TypePath {
                                qself: qself.clone(),
                                path: path.clone(),
                            },
                        );
                    }
                }
                "path" => {
                    let Meta::NameValue(MetaNameValue { value, .. }) = meta else {
                        return Err(syn::Error::new(meta.span(), "not list"));
                    };
                    let Expr::Path(ExprPath { path, .. }) = value else {
                        return Err(syn::Error::new(value.span(), "not path"));
                    };

                    if path
                        .segments
                        .first()
                        .ok_or_else(|| syn::Error::new(path.span(), "no path segments"))?
                        .ident
                        .to_string()
                        != "crate"
                    {
                        return Err(syn::Error::new(
                            path.span(),
                            "path attribute is not absolute",
                        ));
                    }

                    attr.path = Some(path);
                }
                "macro_match" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.macro_match = public_item;
                }
                "macro_construct" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.macro_construct = public_item;
                }
                "into_trait" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.into_trait = public_item;
                }
                "into_method" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.into_method = public_item;
                }
                "try_from_method" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.try_from_method = public_item;
                }
                "try_from_owned_method" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.try_from_owned_method = public_item;
                }
                "try_from_mut_method" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.try_from_mut_method = public_item;
                }
                "extract_method" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.extract_method = public_item;
                }
                "extract_owned_method" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.extract_owned_method = public_item;
                }
                "extract_mut_method" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.extract_mut_method = public_item;
                }
                "try_from_error" => {
                    let MetaList { tokens, .. } = meta.require_list()?;
                    let public_item: PublicItem = parse2(tokens.clone())?;
                    attr.try_from_error = public_item;
                }
                _ => {}
            }
        }
        Ok(attr)
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
