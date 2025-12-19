use heck::ToSnakeCase;
use proc_macro::TokenStream;
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
use syn::Token;
use syn::Type;
use syn::Visibility;
use syn::braced;
use syn::parenthesized;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;
use syn::spanned::Spanned;

const INTERNAL_IDENT_STRING: &str = "__INTERNAL_IDENT_STRING";
const INTERNAL_PATTERN_STRING: &str = "__INTERNAL_PATTERN_STRING";

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
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
    Never,
    Ident(String, Vec<NiceType<P>>),
    Literal(NiceTypeLit),
    PatternIdent(P),
}

impl NiceType<Infallible> {
    fn from_type(ty: &Type) -> Option<Self> {
        match ty {
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
            _ => None,
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

    fn patterns_matching(&self) -> BTreeSet<NiceType<()>> {
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
                            let NiceType::Ident(_name, tys) = &mut out else {
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
            NiceType::Never => "Never".to_string(),
            NiceType::Ident(name, tys) => format!(
                "{}{}",
                name,
                tys.iter()
                    .map(|ty| format!("_{}", Self::variant_name(ty)))
                    .collect::<Vec<_>>()
                    .join("")
            ),
            NiceType::Literal(lit) => lit.variant_name(),
            Self::PatternIdent(x) => x.absurd(),
        }
    }
}

impl<P> NiceType<P> {
    fn index_patterns(&self) -> NiceType<Ident> {
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
                NiceType::PatternIdent(format_ident!(
                    "{}{}",
                    INTERNAL_PATTERN_STRING,
                    i.to_string()
                ))
            }
        }
    }
}

impl<P: Eq> PartialOrd for NiceType<P> {
    fn partial_cmp(&self, other: &NiceType<P>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// those with more patternidents are larger
impl<P: Eq> Ord for NiceType<P> {
    fn cmp(&self, other: &NiceType<P>) -> Ordering {
        match (self, other) {
            (_, NiceType::PatternIdent(_)) => Ordering::Greater,
            (NiceType::PatternIdent(_), _) => Ordering::Less,
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
    fn map_pattern<Q>(&self, f: impl Fn(&P) -> Q + Clone) -> NiceType<Q> {
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
        tokens.append_all(match self {
            Self::Never => quote! { ! },
            Self::Ident(name, tys) => {
                let name = format_ident!("{}", name);
                quote! { #name < #(#tys),* > }
            }
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

    fn macro_internal_name(&self, which: &str) -> Ident {
        Ident::new(
            &format!(
                "{INTERNAL_IDENT_STRING}_{}_{}",
                self.name.to_string().to_snake_case(),
                which
            ),
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
        let macro_match = self.macro_match_name();
        let macro_match_body = self.macro_internal_name("body");
        let macro_match_pattern = self.macro_internal_name("pattern");

        quote! {
            macro_rules! #macro_match {
                ( match $( $rest:tt )* ) => {
                    foo_match_body! { (), ( $($rest)* ) }
                };
            }
        }
    }

    fn macro_match_body(&self) -> proc_macro2::TokenStream {
        let macro_match_body = self.macro_internal_name("body");
        let macro_match_process_body = self.macro_internal_name("process_body");

        quote! {
            macro_rules! #macro_match_body {
                ( $what:tt, ({
                    $( $rest:tt )*
                }) ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), () )
                };
                ( ( $( $what:tt )* ), ( $next:tt $( $rest:tt )* ) ) => {
                    #macro_match_body ! { ( $($what)* $next ), ( $($rest)* ) }
                };
            }
        }
    }

    fn macro_match_process_body(&self) -> proc_macro2::TokenStream {
        let macro_process_body = self.macro_internal_name("process_body");
        let macro_match_pattern = self.macro_internal_name("pattern");
        let macro_match_variant = self.macro_internal_name("variant");

        quote! {
            macro_rules! #macro_process_body {
                ( $what:tt, (), ( $( ( $tyn:ident $(, $( $ty:tt ),* )?; $binding:pat => $body:expr ) )* ) ) => {
                    {
                        let what = $what;

                        #[allow(unreachable_patterns)]
                        match what {
                            $( #macro_match_pattern !($tyn $(, $($ty),* )?) => (), )*
                        }

                        #[allow(unused_labels)]
                        'ma: {
                            $( #macro_match_variant !{$tyn $(, $($ty),* )?; what, 'ma, $binding => $body} )*
                            unreachable!();
                        }
                    }
                };
                (
                    $what:tt,
                    ( $tyn:ident $( ::< $($ty:tt),* $(,)? > )? ( $binding:pat ) => { $( $body:tt )* } $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_process_body !( $what, ( $($rest)* ), ( $($matched)* ( $tyn $(, $($ty),* )?; $binding => { $($body)* } ) ) )
                };
                (
                    $what:tt,
                    ( $tyn:ident $( ::< $($ty:tt),* $(,)? > )? ( $binding:pat ) => $body:expr, $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_process_body !( $what, ( $($rest)* ), ( $($matched)* ( $tyn $(, $($ty),* )?; $binding => $body ) ) )
                };
            }
        }
    }

    fn macro_match_variant(&self) -> proc_macro2::TokenStream {
        let macro_match_variant = self.macro_internal_name("variant");

        quote! {
            macro_rules! #macro_match_variant {
                ( Foo, (A<0>); $what:ident, $ma:lifetime, $binding:pat => $body:expr ) => {
                    if let FooEnum::A_0($binding) = $what {
                        break $ma($body);
                    }
                };
                ( Foo, (A<1>); $what:ident, $ma:lifetime, $binding:pat => $body:expr ) => {
                    if let FooEnum::A_0($binding) = $what {
                        break $ma($body);
                    }
                };
                ( Foo, (A<$N:tt>); $what:ident, $ma:lifetime, $binding:pat => $body:expr ) => {
                    if let FooEnum::A_0($binding) = $what {
                        let $N = 0;
                        break $ma($body);
                    }
                    if let FooEnum::A_1($binding) = $what {
                        let $N = 1;
                        break $ma($body);
                    }
                };
            }
        }
    }

    fn macro_match_pattern(&self) -> proc_macro2::TokenStream {
        let name = &self.name;
        let macro_match_pattern = self.macro_internal_name("pattern");
        let mut patterns_map = BTreeMap::new();
        for ty in &self.tys {
            for pat in ty.patterns_matching() {
                let matches = patterns_map.entry(pat).or_insert(Vec::new());
                matches.push(ty);
            }
        }

        // let mut patterns: Vec<_> = patterns.into_iter().collect();
        // patterns.sort_by_key(|(_, t)| t.len());

        let (pat_names, pat_params): (Vec<_>, Vec<_>) = patterns_map
            .keys()
            .map(|pat| {
                let NiceType::Ident(name, params) = pat.map_pattern(|_| quote! { _ }) else {
                    panic!("not ident");
                };
                (name, params)
            })
            .unzip();
        let tys: Vec<_> = patterns_map.values().collect();

        quote! {
            #[doc(hidden)]
            macro_rules! #macro_match_pattern {
                #( ( #pat_names #( , #pat_params )* ) => {
                    #( #name : #tys (_) )|*
                }; )*
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
            tys.push(
                NiceType::from_type(&ty).ok_or(syn::Error::new(ty.span(), "type is not nice"))?,
            );
        }

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
    let enum_out = sigma_enum.enum_out();
    let macro_match = sigma_enum.macro_match();
    let macro_match_body = sigma_enum.macro_match_body();
    let macro_match_process_body = sigma_enum.macro_match_process_body();
    let macro_match_variant = sigma_enum.macro_match_variant();
    let macro_match_pattern = sigma_enum.macro_match_pattern();

    // Hand the output tokens back to the compiler
    TokenStream::from(quote! {
        #enum_out
        #macro_match
        #macro_match_body
        #macro_match_process_body
        #macro_match_variant
        #macro_match_pattern
    })
}
