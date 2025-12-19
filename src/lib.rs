use heck::ToSnakeCase;
use proc_macro::TokenStream;
use quote::ToTokens;
use quote::TokenStreamExt;
use quote::format_ident;
use quote::quote;
use std::collections::HashSet;
use syn::Expr;
use syn::GenericArgument;
use syn::Ident;
use syn::Lit;
use syn::Token;
use syn::Type;
use syn::Visibility;
use syn::braced;
use syn::parenthesized;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;

const INTERNAL_IDENT_STRING: &str = "__INTERNAL_IDENT_STRING";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Infallible(std::convert::Infallible);

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NiceTypeLit {
    Int(String),
    Bool(bool),
}

impl NiceTypeLit {
    fn from_lit(lit: &Lit) -> Option<Self> {
        match lit {
            Lit::Byte(_lit_byte) => None, // TODO: char and byte
            Lit::Char(_lit_char) => None,
            Lit::Int(lit_int) => Some(Self::Int(lit_int.base10_digits().to_string())),
            Lit::Bool(lit_bool) => Some(Self::Bool(lit_bool.value)),
            _ => return None,
        }
    }

    fn variant_name(&self) -> String {
        match self {
            NiceTypeLit::Int(digits) => digits.replace("-", "Neg"),
            NiceTypeLit::Bool(b) => b.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NiceType<P> {
    Array(Box<NiceType<P>>, Box<NiceType<P>>),
    Never,
    Ident(String, Vec<NiceType<P>>),
    PtrConst(Box<NiceType<P>>),
    PtrMut(Box<NiceType<P>>),
    Tuple(Vec<NiceType<P>>),
    Literal(NiceTypeLit),
    PatternIdent(P),
}

impl NiceType<Infallible> {
    fn from_type(ty: &Type) -> Option<Self> {
        match ty {
            Type::Array(type_array) => {
                let Expr::Lit(len) = &type_array.len else {
                    return None;
                };
                Some(Self::Literal(NiceTypeLit::from_lit(&len.lit)?))
            }
            Type::Never(_type_never) => Some(Self::Never),
            Type::Paren(type_paren) => Self::from_type(&type_paren.elem),
            Type::Path(type_path) => {
                if type_path.qself.is_some() {
                    return None;
                }
                let mut segments_iter = type_path.path.segments.iter();
                match (segments_iter.next(), segments_iter.next()) {
                    (_, Some(_)) | (None, _) => return None,
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
                                        GenericArgument::Const(_) => return None,
                                        _ => return None,
                                    };
                                }
                            }
                            _ => return None,
                        };

                        Some(Self::Ident(segment.ident.to_string(), tys))
                    }
                }
            }
            Type::Ptr(type_ptr) => match type_ptr.mutability {
                Some(_) => Some(Self::PtrMut(Box::new(Self::from_type(&type_ptr.elem)?))),
                None => Some(Self::PtrConst(Box::new(Self::from_type(&type_ptr.elem)?))),
            },
            Type::Tuple(type_tuple) => {
                let mut tys = Vec::new();
                for ty in &type_tuple.elems {
                    tys.push(Self::from_type(&ty)?);
                }
                Some(Self::Tuple(tys))
            }
            _ => None,
        }
    }

    fn words(&self) -> HashSet<String> {
        let mut ws = HashSet::new();
        match self {
            Self::Array(ty, _len) => ws.extend(ty.words()),
            Self::Never => (),
            Self::Ident(name, tys) => {
                ws.insert(name.to_string());
                ws.extend(tys.iter().flat_map(|ty| ty.words()));
            }
            Self::PtrConst(ty) => ws.extend(ty.words()),
            Self::PtrMut(ty) => ws.extend(ty.words()),
            Self::Tuple(tys) => ws.extend(tys.iter().flat_map(|ty| ty.words())),
            Self::Literal(_lit) => (),
            Self::PatternIdent(x) => x.absurd(),
        }
        ws
    }

    fn matches<P>(&self, pat: &NiceType<P>) -> bool {
        match (self, pat) {
            (_, NiceType::PatternIdent(_)) => true,
            (Self::Array(ty, len), NiceType::Array(pat_ty, pat_len)) => {
                ty.matches(pat_ty) && len.matches(pat_len)
            }
            (Self::Never, NiceType::Never) => true,
            (Self::Ident(name, tys), NiceType::Ident(pat_name, pat_tys)) => {
                name == pat_name
                    && zip_equal(tys, pat_tys)
                        .all(|typ| typ.is_ok_and(|(ty, pat_ty)| ty.matches(pat_ty)))
            }
            (Self::PtrConst(ty), NiceType::PtrConst(pat_ty)) => ty.matches(pat_ty),
            (Self::PtrMut(ty), NiceType::PtrMut(pat_ty)) => ty.matches(pat_ty),
            (Self::Tuple(tys), NiceType::Tuple(pat_tys)) => {
                zip_equal(tys, pat_tys).all(|typ| typ.is_ok_and(|(ty, pat_ty)| ty.matches(pat_ty)))
            }
            (Self::Literal(lit), NiceType::Literal(pat_lit)) => lit == pat_lit,
            _ => false,
        }
    }

    fn with_pattern(self: &Box<Self>) -> Box<NiceType<()>> {
        Box::new(self.map_pattern(|_| ()))
    }

    fn to_pattern(&self, words: &HashSet<String>) -> NiceType<()> {
        match self {
            Self::Array(ty, len) => NiceType::Array(
                Box::new(ty.to_pattern(words)),
                Box::new(len.to_pattern(words)),
            ),
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
            Self::PtrConst(ty) => NiceType::PtrConst(Box::new(ty.to_pattern(words))),
            Self::PtrMut(ty) => NiceType::PtrConst(Box::new(ty.to_pattern(words))),
            Self::Tuple(tys) => {
                NiceType::Tuple(tys.iter().map(|ty| ty.to_pattern(words)).collect())
            }
            Self::Literal(lit) => NiceType::Literal(lit.clone()),
            Self::PatternIdent(x) => x.absurd(),
        }
    }

    fn patterns_matching(&self) -> HashSet<NiceType<()>> {
        let mut pats = HashSet::from_iter([self.map_pattern(|_| ()), NiceType::PatternIdent(())]);
        match self {
            Self::Array(ty, len) => pats.extend([
                NiceType::Array(ty.with_pattern(), Box::new(NiceType::PatternIdent(()))),
                NiceType::Array(Box::new(NiceType::PatternIdent(())), len.with_pattern()),
            ]),
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
                            let NiceType::Ident(_name, tys) = &mut out else {
                                unreachable!();
                            };
                            tys.push(new_pattern.map_pattern(|_| ()));
                            out
                        })
                        .collect::<Vec<_>>() // why collect
                }));
            }
            Self::PtrConst(_ty) => {
                pats.extend([NiceType::PtrConst(Box::new(NiceType::PatternIdent(())))])
            }
            Self::PtrMut(_ty) => {
                pats.extend([NiceType::PtrMut(Box::new(NiceType::PatternIdent(())))])
            }
            Self::Tuple(_tys) => 'i: {
                let mut tuple = self.clone();
                let NiceType::Tuple(tys) = &mut tuple else {
                    unreachable!();
                };
                let Some(last) = tys.pop() else {
                    break 'i ();
                };
                let tuple_patterns = tuple.patterns_matching();
                pats.extend(tuple_patterns.iter().flat_map(|tuple_pattern| {
                    last.patterns_matching()
                        .iter()
                        .map(|new_pattern| {
                            let mut out = tuple_pattern.clone();
                            let NiceType::Tuple(tys) = &mut out else {
                                unreachable!();
                            };
                            tys.push(new_pattern.map_pattern(|_| ()));
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

    fn variant_name(&self) -> String {
        match self {
            NiceType::Array(ty, len) => {
                format!(
                    "Array_{}_{}",
                    Self::variant_name(ty),
                    if let Self::Literal(NiceTypeLit::Int(digits)) = &**len {
                        digits
                    } else {
                        panic!("not int")
                    }
                )
            }
            NiceType::Never => "Never".to_string(),
            NiceType::Ident(name, tys) => format!(
                "{}{}",
                name,
                tys.iter()
                    .map(|ty| format!("_{}", Self::variant_name(ty)))
                    .collect::<Vec<_>>()
                    .join("")
            ),
            NiceType::PtrConst(ty) => format!("PtrConst_{}", Self::variant_name(ty)),
            NiceType::PtrMut(ty) => format!("PtrMut_{}", Self::variant_name(ty)),
            NiceType::Tuple(tys) => tys
                .iter()
                .map(|ty| format!("_{}", Self::variant_name(ty)))
                .collect::<Vec<_>>()
                .join("_"),
            NiceType::Literal(lit) => lit.variant_name(),
            Self::PatternIdent(x) => x.absurd(),
        }
    }
}

impl<P: Copy> NiceType<P> {
    fn map_pattern<Q>(&self, f: fn(P) -> Q) -> NiceType<Q> {
        match self {
            Self::Array(ty, len) => {
                NiceType::Array(Box::new(ty.map_pattern(f)), Box::new(len.map_pattern(f)))
            }
            Self::Never => NiceType::Never,
            Self::Ident(name, tys) => NiceType::Ident(
                name.clone(),
                tys.iter().map(|ty| ty.map_pattern(f)).collect(),
            ),
            Self::PtrConst(ty) => NiceType::PtrConst(Box::new(ty.map_pattern(f))),
            Self::PtrMut(ty) => NiceType::PtrMut(Box::new(ty.map_pattern(f))),
            Self::Tuple(tys) => NiceType::Tuple(tys.iter().map(|ty| ty.map_pattern(f)).collect()),
            Self::Literal(lit) => NiceType::Literal(lit.clone()),
            Self::PatternIdent(p) => NiceType::PatternIdent(f(*p)),
        }
    }
}

impl<P: ToTokens> ToTokens for NiceType<P> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.append_all(match self {
            Self::Array(ty, len) => quote! { [#ty, #len] },
            Self::Never => quote! { ! },
            Self::Ident(name, tys) => {
                let name = format_ident!("{}", name);
                quote! { #name < #(#tys),* > }
            }
            Self::PtrConst(ty) => quote! { *const #ty },
            Self::PtrMut(ty) => quote! { *mut #ty },
            Self::Tuple(tys) => quote! { ( #(#tys),* ) },
            Self::Literal(NiceTypeLit::Int(digits)) => digits.parse().unwrap(),
            Self::Literal(NiceTypeLit::Bool(b)) => quote! { #b },
            Self::PatternIdent(p) => {
                p.to_tokens(tokens);
                return;
            }
        });
    }
}

#[derive(Clone)]
struct SigmaEnum {
    visibility: Visibility,
    name: Ident,
    tys: Vec<NiceType<Infallible>>,
}

impl SigmaEnum {
    fn variant_names(&self) -> Vec<Ident> {
        self.tys
            .iter()
            .map(|ty| Ident::new(&ty.variant_name(), self.name.span()))
            .collect()
    }

    fn macro_match_name(&self) -> Ident {
        Ident::new(
            &format!("{}_match", self.name.to_string().to_snake_case()),
            self.name.span(),
        )
    }

    fn macro_match_variant_name(&self) -> Ident {
        Ident::new(
            &format!("{}_match_variant", self.name.to_string().to_snake_case()),
            self.name.span(),
        )
    }

    fn macro_match_pattern_name(&self) -> Ident {
        Ident::new(
            &format!("{}_match_pattern", self.name.to_string().to_snake_case()),
            self.name.span(),
        )
    }

    fn enum_out(&self) -> proc_macro2::TokenStream {
        let SigmaEnum {
            visibility,
            name,
            tys,
        } = &self;
        let variant_names = self.variant_names();

        quote! {
            #visibility enum #name {
                #(#variant_names(#tys),)*
            }
        }
    }

    fn macro_match(&self) -> proc_macro2::TokenStream {
        let macro_match_name = self.macro_match_name();
        let macro_match_variant_name = self.macro_match_variant_name();
        let macro_match_pattern_name = self.macro_match_pattern_name();

        quote! {
            macro_rules! #macro_match_name {
                ( $what:expr, {
                    $( $tyn:ident ::< $($ty:tt),* $(,)? > ( $binding:pat ) => $body:expr ),* $(,)?
                } ) => {
                    {
                        let what = $what;

                        #[allow(unreachable_patterns)]
                        match what {
                            $( #macro_match_pattern_name !($($ty),*) => (), )*
                        }

                        #[allow(unused_labels)]
                        'ma: {
                            $( #macro_match_variant_name !{$($ty),*; what, 'ma, $binding => $body} )*
                            panic!("no match");
                        }
                    }
                };
            }
        }
    }

    fn macro_match_variant(&self) -> proc_macro2::TokenStream {
        let macro_match_variant_name = self.macro_match_variant_name();
        let macro_match_pattern_name = self.macro_match_pattern_name();

        todo!()
    }

    fn macro_match_pattern(&self) -> proc_macro2::TokenStream {
        let macro_match_pattern_name = self.macro_match_pattern_name();

        quote! {
            macro_rules! #macro_match_pattern_name {
                ( (A<0>) ) => {
                    FooEnum::A_0(_)
                };
                ( (A<1>) ) => {
                    FooEnum::A_1(_)
                };
                ( (A<$N:tt>) ) => {
                    FooEnum::A_0(_) | FooEnum::A_1(_)
                };
            }
        }
    }
}

impl Parse for SigmaEnum {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let visibility: Visibility = input.parse()?;
        input.parse::<Token![enum]>()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let mut tys = Vec::new();
        while content.peek(Ident) {
            let _var_name: Ident = content.parse()?;
            let ty_paren;
            parenthesized!(ty_paren in content);
            let ty: Type = ty_paren.parse()?;
            assert!(ty_paren.is_empty());
            // tys.push(ty);
        }

        todo!();
        Ok(SigmaEnum {
            visibility,
            name,
            tys,
        })
    }
}

#[proc_macro_attribute]
pub fn sigma_type(_input: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let sigma_enum = parse_macro_input!(item as SigmaEnum);

    // Build the output, possibly using quasi-quotation

    // Hand the output tokens back to the compiler
    TokenStream::from(quote! {})
}
