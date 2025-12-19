use crate::nice_type::Infallible;
use crate::nice_type::NiceType;
use heck::ToSnakeCase;
use proc_macro::TokenStream;
use quote::ToTokens;
use quote::TokenStreamExt;
use quote::format_ident;
use quote::quote;
use std::collections::BTreeMap;
use syn::Ident;
use syn::Token;
use syn::Type;
use syn::Visibility;
use syn::braced;
use syn::parenthesized;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;
use syn::spanned::Spanned;

mod nice_type;

const INTERNAL_IDENT: &str = "__INTERNAL_IDENT";
const INTERNAL_FULL_WILDCARD: &str = "__INTERNAL_FULL_WILDCARD";

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
            .map(|ty| format_ident!("{}", ty.variant_name()))
            .collect()
    }

    fn macro_match_name(&self) -> Ident {
        format_ident!("{}_match", self.name.to_string().to_snake_case())
    }

    fn macro_internal_name(&self, which: &str) -> Ident {
        format_ident!(
            "{INTERNAL_IDENT}_{}_{}",
            self.name.to_string().to_snake_case(),
            which
        )
    }
}

impl ToTokens for SigmaEnum {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let SigmaEnum {
            visibility,
            name,
            tys,
        } = &self;
        let variant_names = self.variant_names();

        let macro_match = self.macro_match_name();
        let macro_match_body = self.macro_internal_name("body");
        let macro_match_process_body = self.macro_internal_name("process_body");
        let macro_match_variant = self.macro_internal_name("variant");
        let macro_match_pattern = self.macro_internal_name("pattern");

        let internal_full_wildcard = format_ident!("{INTERNAL_FULL_WILDCARD}");

        let mut patterns_map = BTreeMap::new();
        for ty in tys {
            for pat in ty.patterns_matching() {
                let matches = patterns_map.entry(pat).or_insert(Vec::new());
                matches.push(ty);
            }
        }

        // panic!("{:?}", patterns_map);

        let patterns: Vec<_> = patterns_map.keys().collect();
        let pat_variants: Vec<_> = patterns_map.values().collect();
        let pat_variant_names: Vec<Vec<_>> = pat_variants
            .iter()
            .map(|v| v.iter().map(|ty| ty.variant_name()).collect())
            .collect();

        let patterns_vars: Vec<_> = patterns.iter().map(|pat| pat.index_patterns()).collect();
        let pat_variant_assocs: Vec<Vec<BTreeMap<_, _>>> = pat_variants
            .iter()
            .zip(&patterns_vars)
            .map(|(v, pat)| {
                v.iter()
                    .map(|ty| {
                        ty.matches_map(&pat)
                            .into_iter()
                            .filter_map(|(p, ty)| {
                                if let NiceType::Literal(lit) = ty {
                                    Some((p, lit))
                                } else {
                                    None
                                }
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        let pat_variant_assocs_keys: Vec<Vec<Vec<_>>> = pat_variant_assocs
            .iter()
            .map(|v| v.iter().map(|v| v.keys().collect()).collect())
            .collect();
        let pat_variant_assocs_values: Vec<Vec<Vec<_>>> = pat_variant_assocs
            .iter()
            .map(|v| v.iter().map(|v| v.values().collect()).collect())
            .collect();
        let (pat_vars_names, pat_vars_params): (Vec<_>, Vec<_>) = patterns_vars
            .iter()
            .map(|pat| match pat {
                NiceType::Ident(name, params) => (
                    format_ident!("{}", name),
                    params
                        .iter()
                        .map(|param| {
                            let param = param.map_pattern(|p| quote! { ? $ #p :ident });
                            match param {
                                NiceType::PatternIdent(_) => {
                                    param.map_pattern(|p| quote! { ( #p ) })
                                }
                                param => param,
                            }
                        })
                        .collect(),
                ),
                NiceType::PatternIdent(_p) => (internal_full_wildcard.clone(), Vec::new()),
                _ => panic!("not ident {:?}", pat),
            })
            .unzip();

        tokens.append_all(quote! {
            #visibility enum #name {
                #(#variant_names(#tys),)*
            }
        });

        tokens.append_all(quote! {
            macro_rules! #macro_match {
                ( match $( $rest:tt )* ) => {
                    #macro_match_body ! { (), ( $($rest)* ) }
                };
            }
        });

        tokens.append_all(quote! {
            #[doc(hidden)]
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
        });

        tokens.append_all(quote! {
            #[doc(hidden)]
            macro_rules! #macro_match_process_body {
                ( $what:tt, (), ( $( ( $tyn:ident $(, $( $ty:tt ),* )?; $binding:pat => $body:expr ) )* ) ) => {
                    {
                        let what = $what;

                        #[allow(unreachable_patterns)]
                        match what {
                            $( #macro_match_pattern !($tyn $(, $($ty),* )?) => (), )*
                        };

                        #[allow(unused_labels)]
                        'ma: {
                            $( #macro_match_variant !{$tyn $(, $($ty),* )?; what; 'ma; $binding => $body} )*
                            unreachable!();
                        }
                    }
                };
                (
                    $what:tt,
                    ( $tyn:ident $( ::< $($ty:tt),* $(,)? > )? ( $binding:pat ) => { $( $body:tt )* } $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( $tyn $(, $($ty),* )?; $binding => { $($body)* } ) ) )
                };
                (
                    $what:tt,
                    ( $tyn:ident $( ::< $($ty:tt),* $(,)? > )? ( $binding:pat ) => $body:expr, $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( $tyn $(, $($ty),* )?; $binding => $body ) ) )
                };
                (
                    $what:tt,
                    ( $binding:pat => $body:expr, $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( #internal_full_wildcard ; $binding => $body ) ) )
                };
            }
        });

        tokens.append_all(quote! {
            #[doc(hidden)]
            macro_rules! #macro_match_variant {
                #( ( #pat_vars_names #( , #pat_vars_params ),*; $what:ident; $ma:lifetime; $binding:pat => $body:expr ) => {
                    #( if let #name :: #pat_variant_names ($binding) = $what {
                        #( let #pat_variant_assocs_keys = #pat_variant_assocs_values; )*
                        break $ma($body);
                    } )*
                }; )*
            }
        });

        tokens.append_all(quote! {
            #[doc(hidden)]
            macro_rules! #macro_match_pattern {
                #( ( #pat_vars_names #( , #pat_vars_params )* ) => {
                    #( #name :: #pat_variant_names (_) )|*
                }; )*
            }
        });
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
            let _ = content.parse::<Token![,]>();
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

    // panic!("{}", quote! { #sigma_enum });
    quote! { #sigma_enum }.into()
}
