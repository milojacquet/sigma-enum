use quote::ToTokens;
use quote::TokenStreamExt;
use quote::format_ident;
use quote::quote;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use syn::Expr;
use syn::GenericArgument;
use syn::Ident;
use syn::Lit;
use syn::Type;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::spanned::Spanned;

const INTERNAL_PATTERN: &str = "__INTERNAL_PATTERN";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Infallible(std::convert::Infallible);

impl Infallible {
    fn absurd<T>(&self) -> T {
        match self.0 {}
    }
}

impl ToTokens for Infallible {
    fn to_tokens(&self, _: &mut proc_macro2::TokenStream) {
        match self.0 {}
    }
}

fn zip_equal<A, B>(
    a_iter: impl IntoIterator<Item = A>,
    b_iter: impl IntoIterator<Item = B>,
) -> impl Iterator<Item = Result<(A, B), Result<A, B>>> {
    let mut a_iter = a_iter.into_iter();
    let mut b_iter = b_iter.into_iter();
    std::iter::from_fn(move || match (a_iter.next(), b_iter.next()) {
        (None, None) => None,
        (None, Some(b)) => Some(Err(Err(b))),
        (Some(a), None) => Some(Err(Ok(a))),
        (Some(a), Some(b)) => Some(Ok((a, b))),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum NiceTypeLit {
    Int(String),
    Bool(bool),
}

impl NiceTypeLit {
    pub fn from_lit(lit: &Lit) -> syn::Result<Self> {
        match lit {
            // Lit::Byte(_lit_byte) => None, // TODO: char and byte
            // Lit::Char(_lit_char) => None,
            Lit::Int(lit_int) => Ok(Self::Int(lit_int.base10_digits().to_string())),
            Lit::Bool(lit_bool) => Ok(Self::Bool(lit_bool.value)),
            _ => Err(syn::Error::new(lit.span(), "invalid literal")),
        }
    }

    fn variant_name_string(&self) -> String {
        match self {
            NiceTypeLit::Int(digits) => digits.replace("-", "Neg"),
            NiceTypeLit::Bool(b) => b.to_string(),
        }
    }
}

impl ToTokens for NiceTypeLit {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.append_all(match self {
            NiceTypeLit::Int(digits) => digits.parse().unwrap(),
            NiceTypeLit::Bool(b) => quote! { #b },
        });
    }
}

impl Parse for NiceTypeLit {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let lit = input.parse()?;
        Self::from_lit(&lit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NiceType<P> {
    Never,
    Ident(String, Vec<NiceType<P>>),
    Literal(NiceTypeLit),
    PatternIdent(P),
}

impl NiceType<Infallible> {
    pub fn from_type(ty: &Type) -> syn::Result<Self> {
        match ty {
            Type::Never(_type_never) => Ok(Self::Never),
            Type::Paren(type_paren) => Self::from_type(&type_paren.elem),
            Type::Path(type_path) => {
                if type_path.qself.is_some() {
                    return Err(syn::Error::new(type_path.span(), "invalid path"));
                }
                let mut segments_iter = type_path.path.segments.iter();
                match (segments_iter.next(), segments_iter.next()) {
                    (_, Some(_)) | (None, _) => Err(syn::Error::new(
                        type_path.span(),
                        "invalid path (:: not allowed)",
                    )),
                    (Some(segment), _) => {
                        let mut tys = Vec::new();
                        match &segment.arguments {
                            syn::PathArguments::None => {}
                            syn::PathArguments::AngleBracketed(args) => {
                                for arg in &args.args {
                                    match arg {
                                        GenericArgument::Type(ty) => {
                                            tys.push(Self::from_type(&ty)?)
                                        }
                                        GenericArgument::Const(Expr::Lit(lit)) => tys
                                            .push(Self::Literal(NiceTypeLit::from_lit(&lit.lit)?)),
                                        GenericArgument::Const(cons) => {
                                            return Err(syn::Error::new(
                                                cons.span(),
                                                "invalid what",
                                            ));
                                        }
                                        arg => {
                                            return Err(syn::Error::new(
                                                arg.span(),
                                                "invalid what",
                                            ));
                                        }
                                    };
                                }
                            }
                            args => return Err(syn::Error::new(args.span(), "invalid what")),
                        };

                        Ok(Self::Ident(segment.ident.to_string(), tys))
                    }
                }
            }
            ty => Err(syn::Error::new(ty.span(), "invalid type")),
        }
    }

    fn words(&self) -> BTreeSet<String> {
        let mut ws = BTreeSet::new();
        match self {
            Self::Never => (),
            Self::Ident(name, tys) => {
                ws.insert(name.to_string());
                ws.extend(tys.iter().flat_map(|ty| ty.words()));
            }
            Self::Literal(_lit) => (),
            Self::PatternIdent(x) => x.absurd(),
        }
        ws
    }

    fn matches<P>(&self, pat: &NiceType<P>) -> bool {
        match (self, pat) {
            (_, NiceType::PatternIdent(_)) => true,
            (Self::Never, NiceType::Never) => true,
            (Self::Ident(name, tys), NiceType::Ident(pat_name, pat_tys)) => {
                name == pat_name
                    && zip_equal(tys, pat_tys)
                        .all(|typ| typ.is_ok_and(|(ty, pat_ty)| ty.matches(pat_ty)))
            }
            (Self::Literal(lit), NiceType::Literal(pat_lit)) => lit == pat_lit,
            _ => false,
        }
    }

    pub fn matches_map<P: Ord + Clone>(
        &self,
        pat: &NiceType<P>,
    ) -> BTreeMap<P, (Self, Option<(Ident, usize)>)> {
        self.matches_map_ancestor(pat, None)
    }

    fn matches_map_ancestor<P: Ord + Clone>(
        &self,
        pat: &NiceType<P>,
        location: Option<(Ident, usize)>,
    ) -> BTreeMap<P, (Self, Option<(Ident, usize)>)> {
        // TODO: detect duplicate pat
        match (self, pat) {
            (ty, NiceType::PatternIdent(p)) => {
                BTreeMap::from_iter([(p.clone(), (ty.clone(), location))])
            }
            (Self::Ident(name, tys), NiceType::Ident(_pat_name, pat_tys)) => tys
                .into_iter()
                .zip(pat_tys)
                .enumerate()
                .flat_map(|(i, (ty, pat_ty))| {
                    ty.matches_map_ancestor(pat_ty, Some((format_ident!("{}", name), i)))
                        .into_iter()
                })
                .collect(),
            _ => BTreeMap::new(),
        }
    }

    pub fn replace_ident(&self, find: &String, replace: &NiceTypeLit) -> Self {
        match self {
            Self::Never => Self::Never,
            Self::Ident(name, tys) => {
                if name == find {
                    Self::Literal(replace.clone())
                } else {
                    Self::Ident(
                        name.clone(),
                        tys.iter()
                            .map(|ty| ty.replace_ident(find, replace))
                            .collect(),
                    )
                }
            }
            Self::Literal(lit) => Self::Literal(lit.clone()),
            Self::PatternIdent(x) => x.absurd(),
        }
    }

    fn with_pattern(self: &Box<Self>) -> Box<NiceType<()>> {
        Box::new(self.map_pattern(|_| ()))
    }

    fn to_pattern(&self, words: &BTreeSet<String>) -> NiceType<()> {
        match self {
            Self::Never => NiceType::Never,
            Self::Ident(name, tys) => {
                if words.contains(name) && tys.is_empty() {
                    NiceType::PatternIdent(())
                } else {
                    NiceType::Ident(
                        name.to_string(),
                        tys.iter().map(|ty| ty.to_pattern(words)).collect(),
                    )
                }
            }
            Self::Literal(lit) => NiceType::Literal(lit.clone()),
            Self::PatternIdent(x) => x.absurd(),
        }
    }

    pub fn patterns_matching(&self) -> BTreeSet<NiceType<()>> {
        let mut pats = BTreeSet::from_iter([self.map_pattern(|_| ()), NiceType::PatternIdent(())]);
        match self {
            Self::Never => (),
            Self::Ident(_name, _tys) => 'i: {
                let mut ident = self.clone();
                let NiceType::Ident(_name, tys) = &mut ident else {
                    unreachable!();
                };
                let Some(last) = tys.pop() else {
                    break 'i ();
                };
                let ident_patterns = ident.patterns_matching();
                pats.extend(ident_patterns.iter().flat_map(|ident_pattern| {
                    last.patterns_matching()
                        .iter()
                        .map(|new_pattern| {
                            let mut out = ident_pattern.clone();
                            if let NiceType::Ident(_name, tys) = &mut out {
                                tys.push(new_pattern.map_pattern(|_| ()));
                            }
                            out
                        })
                        .collect::<Vec<_>>() // why collect
                }));
            }
            Self::Literal(_lit) => (),
            Self::PatternIdent(x) => x.absurd(),
        }
        pats
    }

    pub fn variant_name_string(&self) -> String {
        match self {
            NiceType::Never => "Never".to_string(),
            NiceType::Ident(name, tys) => format!(
                "{}{}",
                name,
                tys.iter()
                    .map(|ty| format!("_{}", Self::variant_name_string(ty)))
                    .collect::<Vec<_>>()
                    .join("")
            ),
            NiceType::Literal(lit) => lit.variant_name_string(),
            Self::PatternIdent(x) => x.absurd(),
        }
    }

    pub fn variant_name(&self) -> Ident {
        format_ident!("{}", self.variant_name_string())
    }
}

impl<P> NiceType<P> {
    pub fn index_patterns(&self) -> NiceType<Ident> {
        self.index_patterns_index(&mut 0)
    }

    fn index_patterns_index(&self, i: &mut usize) -> NiceType<Ident> {
        match self {
            Self::Never => NiceType::Never,
            Self::Ident(name, tys) => NiceType::Ident(
                name.clone(),
                tys.iter().map(|ty| ty.index_patterns_index(i)).collect(),
            ),
            Self::Literal(lit) => NiceType::Literal(lit.clone()),
            Self::PatternIdent(_) => {
                *i += 1;
                NiceType::PatternIdent(format_ident!("{}_{}", INTERNAL_PATTERN, i.to_string()))
            }
        }
    }
}

impl<P: Eq + Ord> PartialOrd for NiceType<P> {
    fn partial_cmp(&self, other: &NiceType<P>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// those with more patternidents are larger
impl<P: Eq + Ord> Ord for NiceType<P> {
    fn cmp(&self, other: &NiceType<P>) -> Ordering {
        // PatternIdent should compare greater than everything
        match (self, other) {
            (NiceType::PatternIdent(p), NiceType::PatternIdent(q)) => p.cmp(q),
            (_, NiceType::PatternIdent(_)) => Ordering::Less,
            (NiceType::PatternIdent(_), _) => Ordering::Greater,
            (Self::Never, NiceType::Never) => Ordering::Equal,
            (_, NiceType::Never) => Ordering::Greater,
            (NiceType::Never, _) => Ordering::Less,
            (Self::Ident(name, tys), NiceType::Ident(pat_name, pat_tys)) => {
                name.cmp(&pat_name).then_with(|| tys.cmp(&pat_tys))
            }
            (_, NiceType::Ident(..)) => Ordering::Greater,
            (NiceType::Ident(..), _) => Ordering::Less,
            (Self::Literal(lit), NiceType::Literal(pat_lit)) => lit.cmp(&pat_lit),
        }
    }
}

impl<P> NiceType<P> {
    pub fn map_pattern<Q>(&self, f: impl Fn(&P) -> Q + Clone) -> NiceType<Q> {
        match self {
            Self::Never => NiceType::Never,
            Self::Ident(name, tys) => NiceType::Ident(
                name.clone(),
                tys.iter().map(|ty| ty.map_pattern(f.clone())).collect(),
            ),
            Self::Literal(lit) => NiceType::Literal(lit.clone()),
            Self::PatternIdent(p) => NiceType::PatternIdent(f(p)),
        }
    }
}

impl<P: ToTokens> ToTokens for NiceType<P> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self {
            Self::Never => tokens.append_all(quote! { ! }),
            Self::Ident(name, tys) => {
                let name = format_ident!("{}", name);
                if tys.is_empty() {
                    tokens.append_all(quote! { #name })
                } else {
                    tokens.append_all(quote! { #name < #(#tys),* > })
                }
            }
            Self::Literal(lit) => lit.to_tokens(tokens),
            Self::PatternIdent(p) => p.to_tokens(tokens),
        };
    }
}

impl Parse for NiceType<Infallible> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // TODO: nicer?
        let ty: Type = input.parse()?;
        NiceType::from_type(&ty)
    }
}
