use crate::attrs::extract_expansion;
use crate::nice_type::Infallible;
use crate::nice_type::NiceType;
use attrs::ItemAttr;
use heck::ToSnakeCase;
use nice_type::NiceTypeLit;
use proc_macro::TokenStream;
use quote::ToTokens;
use quote::TokenStreamExt;
use quote::format_ident;
use quote::quote;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use syn::Attribute;
use syn::Expr;
use syn::Ident;
use syn::LitStr;
use syn::Token;
use syn::Visibility;
use syn::braced;
use syn::parenthesized;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;
use syn::spanned::Spanned;

mod attrs;
mod nice_type;

const INTERNAL_IDENT: &str = "__INTERNAL_IDENT";
const INTERNAL_FULL_WILDCARD: &str = "__INTERNAL_FULL_WILDCARD";

#[derive(Clone)]
struct Variant {
    ty: NiceType<Infallible>,
    name: Ident,
    attrs: Vec<Attribute>,
    docs: proc_macro2::TokenStream,
}

#[derive(Clone)]
struct SigmaEnum {
    visibility: Visibility,
    name: Ident,
    variants: Vec<Variant>,
    subattrs: Vec<Attribute>,
    attr: ItemAttr,
}

impl SigmaEnum {
    fn macro_match_name(&self) -> Ident {
        self.attr.macro_match.name.as_ref().map_or_else(
            || format_ident!("{}_match", self.name.to_string().to_snake_case()),
            |name| format_ident!("{}", name),
        )
    }

    fn macro_construct_name(&self) -> Ident {
        self.attr.macro_construct.name.as_ref().map_or_else(
            || format_ident!("{}_construct", self.name.to_string().to_snake_case()),
            |name| format_ident!("{}", name),
        )
    }

    fn into_trait_name(&self) -> Ident {
        self.attr.into_trait.name.as_ref().map_or_else(
            || format_ident!("Into{}", self.name),
            |name| format_ident!("{}", name),
        )
    }

    fn into_method_name(&self) -> Ident {
        self.attr.into_method.name.as_ref().map_or_else(
            || format_ident!("into_{}", self.name.to_string().to_snake_case()),
            |name| format_ident!("{}", name),
        )
    }

    fn try_from_method_name(&self) -> Ident {
        self.attr.try_from_method.name.as_ref().map_or_else(
            || format_ident!("try_from_{}", self.name.to_string().to_snake_case()),
            |name| format_ident!("{}", name),
        )
    }

    fn try_from_owned_method_name(&self) -> Ident {
        self.attr.try_from_owned_method.name.as_ref().map_or_else(
            || format_ident!("try_from_owned_{}", self.name.to_string().to_snake_case()),
            |name| format_ident!("{}", name),
        )
    }

    fn try_from_mut_method_name(&self) -> Ident {
        self.attr.try_from_mut_method.name.as_ref().map_or_else(
            || format_ident!("try_from_mut_{}", self.name.to_string().to_snake_case()),
            |name| format_ident!("{}", name),
        )
    }

    fn extract_method_name(&self) -> Ident {
        self.attr.extract_method.name.as_ref().map_or_else(
            || format_ident!("extract"),
            |name| format_ident!("{}", name),
        )
    }

    fn extract_owned_method_name(&self) -> Ident {
        self.attr.extract_owned_method.name.as_ref().map_or_else(
            || format_ident!("extract_owned"),
            |name| format_ident!("{}", name),
        )
    }

    fn extract_mut_method_name(&self) -> Ident {
        self.attr.extract_mut_method.name.as_ref().map_or_else(
            || format_ident!("extract_mut"),
            |name| format_ident!("{}", name),
        )
    }

    fn try_from_error_name(&self) -> Ident {
        self.attr.try_from_error.name.as_ref().map_or_else(
            || format_ident!("TryFrom{}Error", self.name.to_string()),
            |name| format_ident!("{}", name),
        )
    }

    fn internal_name(&self, which: &str, suffix: &str) -> Ident {
        format_ident!(
            "{INTERNAL_IDENT}_{}_{}{}",
            self.name.to_string().to_snake_case(),
            which,
            suffix
        )
    }

    fn to_tokens_macros(&self, tokens: &mut proc_macro2::TokenStream, export: bool, suffix: &str) {
        let SigmaEnum {
            visibility: _,
            name,
            variants,
            subattrs: _,
            attr,
        } = &self;

        let item_path = match &attr.path {
            Some(path) => quote! { $ #path :: },
            None => quote! {},
        };
        let macro_path = if export {
            quote! { $crate :: }
        } else {
            item_path.clone()
        };

        let variants_btree: BTreeMap<_, _> = variants
            .iter()
            .map(|var| (var.ty.clone(), var.name.clone()))
            .collect();
        let variant_pats: Vec<_> = variants.iter().map(|var| var.ty.clone()).collect();

        let macro_match = format_ident!("{}{}", self.macro_match_name(), suffix);
        let macro_construct = format_ident!("{}{}", self.macro_construct_name(), suffix);
        let macro_match_body = self.internal_name("body", suffix);
        let macro_match_process_body = self.internal_name("process_body", suffix);
        let macro_process_type = self.internal_name("process_type", suffix);
        let macro_match_variant = self.internal_name("variant", suffix);
        let macro_match_pattern = self.internal_name("pattern", suffix);
        let macro_construct_inner = self.internal_name("construct_inner", suffix);

        let macro_match_docstring = self.attr.macro_match.docstring();
        let macro_construct_docstring = self.attr.macro_construct.docstring();

        // https://github.com/rust-lang/rust/pull/52234#issuecomment-1417098097
        let macro_match_export;
        let macro_construct_export;
        let macro_match_body_export;
        let macro_match_process_body_export;
        let macro_process_type_export;
        let macro_match_variant_export;
        let macro_match_pattern_export;
        let macro_construct_inner_export;
        let macro_match_pub_use;
        let macro_construct_pub_use;
        let macro_match_body_pub_use;
        let macro_match_process_body_pub_use;
        let macro_process_type_pub_use;
        let macro_match_variant_pub_use;
        let macro_match_pattern_pub_use;
        let macro_construct_inner_pub_use;
        if export {
            macro_match_export = quote! { #macro_match_docstring #[macro_export] };
            macro_construct_export = quote! { #macro_construct_docstring #[macro_export] };
            macro_match_body_export = quote! { #[macro_export] };
            macro_match_process_body_export = quote! { #[macro_export] };
            macro_process_type_export = quote! { #[macro_export] };
            macro_match_variant_export = quote! { #[macro_export] };
            macro_match_pattern_export = quote! { #[macro_export] };
            macro_construct_inner_export = quote! { #[macro_export] };
            macro_match_pub_use = quote! {};
            macro_construct_pub_use = quote! {};
            macro_match_body_pub_use = quote! {};
            macro_match_process_body_pub_use = quote! {};
            macro_process_type_pub_use = quote! {};
            macro_match_variant_pub_use = quote! {};
            macro_match_pattern_pub_use = quote! {};
            macro_construct_inner_pub_use = quote! {};
        } else {
            macro_match_export = quote! { #macro_match_docstring };
            macro_construct_export = quote! { #macro_construct_docstring };
            macro_match_body_export = quote! {};
            macro_match_process_body_export = quote! {};
            macro_process_type_export = quote! {};
            macro_match_variant_export = quote! {};
            macro_match_pattern_export = quote! {};
            macro_construct_inner_export = quote! {};
            macro_match_pub_use = quote! { #[allow(nonstandard_style)] #[allow(unused_imports)] #macro_match_docstring pub(crate) use #macro_match; };
            macro_construct_pub_use = quote! { #[allow(nonstandard_style)] #[allow(unused_imports)] #macro_construct_docstring pub(crate) use #macro_construct; };
            macro_match_body_pub_use = quote! { #[allow(nonstandard_style)] #[allow(unused_imports)] #[doc(hidden)] pub(crate) use #macro_match_body; };
            macro_match_process_body_pub_use = quote! { #[allow(nonstandard_style)] #[allow(unused_imports)] #[doc(hidden)] pub(crate) use #macro_match_process_body; };
            macro_process_type_pub_use = quote! { #[allow(nonstandard_style)] #[allow(unused_imports)] #[doc(hidden)] pub(crate) use #macro_process_type; };
            macro_match_variant_pub_use = quote! { #[allow(nonstandard_style)] #[allow(unused_imports)] #[doc(hidden)] pub(crate) use #macro_match_variant; };
            macro_match_pattern_pub_use = quote! { #[allow(nonstandard_style)] #[allow(unused_imports)] #[doc(hidden)] pub(crate) use #macro_match_pattern; };
            macro_construct_inner_pub_use = quote! { #[allow(nonstandard_style)] #[allow(unused_imports)] #[doc(hidden)] pub(crate) use #macro_construct_inner; };
        }

        let internal_full_wildcard = format_ident!("{INTERNAL_FULL_WILDCARD}");

        let mut patterns_map = BTreeMap::new();
        patterns_map.insert(NiceType::PatternIdent(()), Vec::new());
        for ty in &variant_pats {
            for pat in ty.patterns_matching() {
                let matches = patterns_map.entry(pat).or_insert(Vec::new());
                matches.push(ty);
            }
        }

        let patterns: Vec<_> = patterns_map.keys().collect();
        let pat_variants: Vec<_> = patterns_map.values().collect();
        let pat_variant_names: Vec<Vec<_>> = pat_variants
            .iter()
            .map(|v| v.iter().map(|ty| variants_btree[ty].clone()).collect())
            .collect();

        let patterns_vars: Vec<_> = patterns.iter().map(|pat| pat.index_patterns()).collect();
        let patterns_vars_assoc: Vec<Vec<Vec<_>>> = pat_variants
            .iter()
            .zip(&patterns_vars)
            .map(|(v, pat)| {
                v.iter()
                    .map(|ty| {
                        ty.matches_map(&pat)
                            .into_iter()
                            .filter_map(|(ident, (ty, location))| {
                                let NiceType::Literal(lit) = ty else {
                                    return None;
                                };
                                // try block. sad
                                let generic_ty = (|| {
                                    let (parent, i) = location?;
                                    self.attr.generics.get(&parent)?[i].as_ref()
                                })();
                                Some((ident, lit, generic_ty))
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        // for each pattern, for each variant it matches, get the type pattern variables
        // and their literals and locations, and generate let statements for them
        let const_let_statements: Vec<Vec<proc_macro2::TokenStream>> = patterns_vars_assoc
            .iter()
            .map(|v| {
                v.iter()
                    .map(|v| {
                        v.iter()
                            .map(|(ident, lit, generic_ty)| match generic_ty {
                                Some(generic_ty) => quote! { const $ #ident : #generic_ty = #lit; },
                                None => quote! { let $ #ident = #lit; },
                            })
                            .map(|q| quote! { #[allow(nonstandard_style)] #[allow(unused_variables)] #q })
                            .collect()
                    })
                    .collect()
            })
            .collect();

        let pat_vars_params_eqs: Vec<Vec<Vec<_>>> = patterns_vars_assoc
            .iter()
            .map(|v| {
                v.iter()
                    .map(|v| {
                        v.iter()
                            .map(|(ident, lit, _generic_ty)| quote! { $ #ident == #lit })
                            .collect()
                    })
                    .collect()
            })
            .collect();

        let (pat_vars_names, pat_vars_params): (Vec<_>, Vec<_>) = patterns_vars
            .iter()
            .map(|pat| match pat {
                NiceType::Ident(name, params) => (format_ident!("{}", name), {
                    let params: Vec<_> = params
                        .iter()
                        .map(|param| param.map_pattern(|p| quote! { ? $ #p :ident }))
                        .collect();
                    (!params.is_empty())
                        .then_some(params)
                        .into_iter()
                        .collect::<Vec<_>>()
                }),
                NiceType::PatternIdent(_p) => (
                    internal_full_wildcard.clone(),
                    None.into_iter().collect::<Vec<_>>(),
                ),
                _ => panic!("not ident {:?}", pat),
            })
            .unzip();

        tokens.append_all(quote! {
            #macro_match_export
            #[allow(unused_macros)]
            macro_rules! #macro_match {
                ( match $( $rest:tt )* ) => {
                    #macro_path #macro_match_body ! { (), ( $($rest)* ) }
                };
            }
            #macro_match_pub_use
        });

        tokens.append_all(quote! {
            #macro_match_body_export
            #[doc(hidden)]
            #[allow(nonstandard_style)]
            macro_rules! #macro_match_body {
                ( $what:tt, ({
                    $( $rest:tt )*
                }) ) => {
                    #macro_path #macro_match_process_body !( $what, ( $($rest)* ), () )
                };
                ( ( $( $what:tt )* ), ( $next:tt $( $rest:tt )* ) ) => {
                    #macro_path #macro_match_body ! { ( $($what)* $next ), ( $($rest)* ) }
                };
            }
            #macro_match_body_pub_use
        });

        tokens.append_all(quote! {
            #macro_match_process_body_export
            #[doc(hidden)]
            #[allow(nonstandard_style)]
            macro_rules! #macro_match_process_body {
                ( $what:tt, (), ( $( ( $ty:tt; $binding:pat => $body:expr ) )* ) ) => {
                    {
                        let what = $what;

                        #[allow(unreachable_patterns)]
                        match what {
                            $( #macro_path #macro_match_pattern !($ty) => (), )*
                        }

                        #[allow(unused_labels)]
                        'ma: {
                            $( #macro_path #macro_match_variant !{$ty; what; 'ma; $binding => $body} )*
                            ::core::unreachable!();
                        }
                    }
                };
                (
                    $what:tt,
                    ( $binding:ident => { $( $body:tt )* } $(,)? $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_path #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( (#internal_full_wildcard) ; $binding => { $( $body )* } ) ) )
                };
                (
                    $what:tt,
                    ( $binding:ident => $body:expr, $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_path #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( (#internal_full_wildcard) ; $binding => { $body } ) ) )
                };
                (
                    $what:tt,
                    ( $tyn:ident ( $binding:pat ) => { $( $body:tt )* } $(,)? $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_path #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( ($tyn); $binding => { $($body)* } ) ) )
                };
                (
                    $what:tt,
                    ( $tyn:ident ( $binding:pat ) => $body:expr, $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_path #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( ($tyn); $binding => { $body } ) ) )
                };
                (
                    $what:tt,
                    ( $tyn:ident ::< $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_path #macro_process_type !( (@match, $what, $tyn, ($( $matched )*)), ($( $rest )*), (<), (<) )
                };
            }
            #macro_match_process_body_pub_use
        });

        tokens.append_all(quote! {
            #macro_process_type_export
            #[doc(hidden)]
            #[allow(nonstandard_style)]
            macro_rules! #macro_process_type {
                ( $bundle:tt, ($(,)? > $($rest:tt)*), ( $($params:tt)* ), (< $($counter:tt)*) ) => {
                    #macro_path #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* >), ($($counter)*) )
                };
                ( $bundle:tt, ($(,)? >> $($rest:tt)*), ( $($params:tt)* ), (< < $($counter:tt)*) ) => {
                    #macro_path #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* > >), ($($counter)*) )
                };
                ( $bundle:tt, ($(,)? > $($rest:tt)*), ( $($params:tt)* ), () ) => {
                    ::core::compile_error!("imbalanced")
                };
                ( $bundle:tt, ($(,)? >> $($rest:tt)*), ( $($params:tt)* ), () ) => {
                    ::core::compile_error!("imbalanced")
                };
                ( $bundle:tt, (< $($rest:tt)*), ( $($params:tt)* ), ( $($counter:tt)* ) ) => {
                    #macro_path #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* <), (< $($counter)*) )
                };
                ( $bundle:tt, (<< $($rest:tt)*), ( $($params:tt)* ), ( $($counter:tt)* ) ) => {
                    #macro_path #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* < <), (< < $($counter)*) )
                };
                ( (@match, $what:tt, $tyn:ident, ( $($matched:tt)* )), (( $binding:pat ) => { $( $body:tt )* } $(,)? $($rest:tt)*), ( $($params:tt)* ), () ) => {
                    #macro_path #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( ($tyn :: $($params)+); $binding => { $($body)* } ) ) )
                };
                ( (@match, $what:tt, $tyn:ident, ( $($matched:tt)* )), (( $binding:pat ) => $body:expr, $($rest:tt)*), ( $($params:tt)* ), () ) => {
                    #macro_path #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( ($tyn :: $($params)+); $binding => { $body } ) ) )
                };
                ( (@construct, $tyn:ident), (( $expr:expr )), ( $($params:tt)+ ), () ) => {
                    #macro_path #macro_construct_inner !( ($tyn :: $($params)+); ( $expr ) )
                };
                ( $bundle:tt, (( $($any:tt)* ) $($rest:tt)*), ( $($params:tt)* ), ( $($counter:tt)* ) ) => {
                    ::core::compile_error!("imbalanced or something")
                };
                ( $bundle:tt, ($thing:tt $($rest:tt)*), ( $($params:tt)* ), ( $($counter:tt)* ) ) => {
                    #macro_path #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* $thing), ( $($counter)*) )
                };
            }
            #macro_process_type_pub_use
        });

        tokens.append_all(quote! {
            #macro_match_variant_export
            #[doc(hidden)]
            #[allow(nonstandard_style)]
            macro_rules! #macro_match_variant {
                #( ( (#pat_vars_names #(::< #( #pat_vars_params ),* >)* ); $what:ident; $ma:lifetime; $binding:pat => $body:expr ) => {
                    #( if let #item_path #name :: #pat_variant_names ($binding) = $what {
                        #const_let_statements
                        break $ma($body);
                    } )*
                }; )*
            }
            #macro_match_variant_pub_use
        });

        tokens.append_all(quote! {
            #macro_match_pattern_export
            #[doc(hidden)]
            #[allow(nonstandard_style)]
            macro_rules! #macro_match_pattern {
                ( ( #internal_full_wildcard ) ) => { _ };
                #( ( ( #pat_vars_names #(::< #( #pat_vars_params ),* >)* ) ) => {
                    #( #item_path #name :: #pat_variant_names (_) )|*
                }; )*
            }
            #macro_match_pattern_pub_use
        });

        tokens.append_all(quote! {
            #macro_construct_export
            #[allow(unused_macros)]
            macro_rules! #macro_construct {
                ( $tyn:ident ::< $($tt:tt)* ) => {
                    #macro_path #macro_process_type !( (@construct, $tyn), ($($tt)*), (<), (<) )
                };
                ( $tyn:ident ( $body:expr ) ) => {
                    #macro_path #macro_construct_inner !( ($tyn); ($body) )
                };
            }
            #macro_construct_pub_use
        });

        tokens.append_all(quote! {
            #macro_construct_inner_export
            #[doc(hidden)]
            #[allow(nonstandard_style)]
            macro_rules! #macro_construct_inner {
                #( ( (#pat_vars_names #(::< #( #pat_vars_params ),* >)* ); $body:expr ) => {
                    'ma: {
                        #( if true #(&& #pat_vars_params_eqs)* {
                            #const_let_statements
                            break 'ma ::core::option::Option::Some(#item_path #name :: #pat_variant_names($body));
                        } )*
                        ::core::option::Option::None
                    }
                }; )*
            }
            #macro_construct_inner_pub_use
        });
    }

    fn to_tokens_traits(&self, tokens: &mut proc_macro2::TokenStream) {
        let SigmaEnum {
            visibility,
            name,
            variants,
            subattrs: _,
            attr,
        } = &self;

        let variant_types: Vec<_> = variants
            .iter()
            .map(|var| var.ty.to_tokens_aliased(&attr.alias))
            .collect();
        let variant_names: Vec<_> = variants.iter().map(|var| var.name.clone()).collect();

        let into_trait = self.into_trait_name();
        let into_trait_sealed_mod = self.internal_name("into_trait_sealed_mod", "");
        let into_method = self.into_method_name();
        let try_from_method = self.try_from_method_name();
        let try_from_owned_method = self.try_from_owned_method_name();
        let try_from_mut_method = self.try_from_mut_method_name();
        let extract_method = self.extract_method_name();
        let extract_owned_method = self.extract_owned_method_name();
        let extract_mut_method = self.extract_mut_method_name();
        let try_from_error = self.try_from_error_name();

        let into_trait_docstring = self.attr.into_trait.docstring();
        let into_method_docstring = self.attr.into_method.docstring();
        let try_from_method_docstring = self.attr.try_from_method.docstring();
        let try_from_owned_method_docstring = self.attr.try_from_owned_method.docstring();
        let try_from_mut_method_docstring = self.attr.try_from_mut_method.docstring();
        let extract_method_docstring = self.attr.extract_method.docstring();
        let extract_owned_method_docstring = self.attr.extract_owned_method.docstring();
        let extract_mut_method_docstring = self.attr.extract_mut_method.docstring();
        let try_from_error_docstring = self.attr.try_from_error.docstring();

        let methods = quote! {
            #into_method_docstring
            fn #into_method (self) -> #name;
            #try_from_method_docstring
            fn #try_from_method (value: & #name) -> ::core::option::Option<&Self>;
            #try_from_owned_method_docstring
            fn #try_from_owned_method (value: #name) -> ::core::option::Option<Self>
                where Self: ::core::marker::Sized;
            #try_from_mut_method_docstring
            fn #try_from_mut_method (value: &mut #name) -> ::core::option::Option<&mut Self>;
        };

        tokens.append_all(quote! {
            #into_trait_docstring
            pub trait #into_trait : #into_trait_sealed_mod ::Sealed {
                #methods
            }

            #[allow(nonstandard_style)]
            mod #into_trait_sealed_mod {
                pub trait Sealed {}
            }

            #(
                #[automatically_derived]
                impl #into_trait_sealed_mod ::Sealed for #variant_types {}
            )*
        });

        tokens.append_all(quote! {
            #(
                #into_trait_docstring
                #[automatically_derived]
                impl #into_trait for #variant_types {
                    fn #into_method (self) -> #name {
                        #name :: #variant_names (self)
                    }

                    fn #try_from_method <'a>(value: &'a #name) -> ::core::option::Option<&'a Self> {
                        if let #name :: #variant_names (out) = value {
                            ::core::option::Option::Some(out)
                        } else {
                            ::core::option::Option::None
                        }
                    }

                    fn #try_from_owned_method (value: #name) -> ::core::option::Option<Self>
                        where Self: ::core::marker::Sized
                    {
                        if let #name :: #variant_names (out) = value {
                            ::core::option::Option::Some(out)
                        } else {
                            ::core::option::Option::None
                        }
                    }

                    fn #try_from_mut_method <'a>(value: &'a mut #name) -> ::core::option::Option<&'a mut Self> {
                        if let #name :: #variant_names (out) = value {
                            ::core::option::Option::Some(out)
                        } else {
                            ::core::option::Option::None
                        }
                    }
                }

                #[automatically_derived]
                impl ::core::convert::From<#variant_types> for #name {
                    fn from(value: #variant_types) -> Self {
                        #into_trait :: #into_method (value)
                    }
                }

                #[automatically_derived]
                impl<'a> ::core::convert::TryFrom<&'a #name> for &'a #variant_types {
                    type Error = #try_from_error;
                    fn try_from(value: &'a #name) -> ::core::result::Result<&'a #variant_types, #try_from_error > {
                       < #variant_types as #into_trait >:: #try_from_method (value).ok_or( #try_from_error )
                    }
                }

                #[automatically_derived]
                impl ::core::convert::TryFrom<#name> for #variant_types
                        where Self: ::core::marker::Sized
                {
                    type Error = #try_from_error;
                    fn try_from(value: #name) -> ::core::result::Result<#variant_types, #try_from_error > {
                       < #variant_types as #into_trait >:: #try_from_owned_method (value).ok_or( #try_from_error )
                    }
                }
            )*

            impl #name {
                #extract_method_docstring
                #visibility fn #extract_method <T: #into_trait >(&self) -> ::core::option::Option<&T> {
                    T:: #try_from_method (self)
                }

                #extract_owned_method_docstring
                #visibility fn #extract_owned_method <T: #into_trait >(self) -> ::core::option::Option<T> {
                    T:: #try_from_owned_method (self)
                }

                #extract_mut_method_docstring
                #visibility fn #extract_mut_method <T: #into_trait >(&mut self) -> ::core::option::Option<&mut T> {
                    T:: #try_from_mut_method (self)
                }
            }
        });

        tokens.append_all(quote! {
            #try_from_error_docstring
            pub struct #try_from_error;

            #[automatically_derived]
            impl ::core::fmt::Debug for #try_from_error {
                #[inline]
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    ::core::fmt::Formatter::write_str(f, ::core::stringify!(#try_from_error))
                }
            }
            #[automatically_derived]
            impl ::core::clone::Clone for #try_from_error {
                #[inline]
                fn clone(&self) -> #try_from_error {
                    *self
                }
            }
            #[automatically_derived]
            impl ::core::marker::Copy for #try_from_error {}
            #[automatically_derived]
            impl ::core::cmp::PartialEq for #try_from_error {
                #[inline]
                fn eq(&self, other: & #try_from_error) -> bool {
                    true
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Eq for #try_from_error {}
            #[automatically_derived]
            impl ::core::hash::Hash for #try_from_error {
                #[inline]
                fn hash<__H: ::core::hash::Hasher>(&self, state: &mut __H) -> () {}
            }
            #[automatically_derived]
            impl ::core::cmp::PartialOrd for #try_from_error {
                #[inline]
                fn partial_cmp(&self, other: & #try_from_error) -> ::core::option::Option<::core::cmp::Ordering> {
                    ::core::option::Option::Some(::core::cmp::Ordering::Equal)
                }
            }
            #[automatically_derived]
            impl ::core::cmp::Ord for #try_from_error {
                #[inline]
                fn cmp(&self, other: & #try_from_error) -> ::core::cmp::Ordering {
                    ::core::cmp::Ordering::Equal
                }
            }

            #[automatically_derived]
            impl ::core::fmt::Display for #try_from_error {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    f.write_str("attempted to extract value from a ")?;
                    f.write_str(::core::stringify!( #name ))?;
                    f.write_str(" holding a different type")?;
                    ::core::fmt::Result::Ok(())
                }
            }

            #[automatically_derived]
            impl ::core::error::Error for #try_from_error {}
        });
    }
}

impl ToTokens for SigmaEnum {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let SigmaEnum {
            visibility,
            name,
            variants,
            subattrs,
            attr,
        } = &self;

        if attr.path.is_none()
            && matches!(
                visibility,
                Visibility::Public(_) | Visibility::Restricted(_)
            )
        {
            tokens.append_all(
                quote! { ::core::compile_error!("public or restricted enum without path attribute"); },
            );
            return;
        }

        let variant_types: Vec<_> = variants
            .iter()
            .map(|var| var.ty.to_tokens_aliased(&attr.alias))
            .collect();
        let variant_names: Vec<_> = variants.iter().map(|var| var.name.clone()).collect();
        let variant_attrs: Vec<_> = variants.iter().map(|var| var.attrs.clone()).collect();
        let variant_docs: Vec<_> = variants.iter().map(|var| var.docs.clone()).collect();

        tokens.append_all(quote! {
            #(#subattrs)*
            #visibility enum #name {
                #(
                    #variant_docs
                    #(#variant_attrs)*
                    #variant_names(#variant_types),
                )*
            }
        });

        match visibility {
            Visibility::Public(_) => {
                self.to_tokens_macros(tokens, true, "");
                self.to_tokens_macros(tokens, false, "_crate");
            }
            _ => {
                self.to_tokens_macros(tokens, false, "");
            }
        }
        self.to_tokens_traits(tokens);
    }
}

fn substitute_template(
    template: &str,
    assignments: &[(Ident, NiceTypeLit)],
) -> syn::Result<String> {
    let mut name = template.to_string();
    for (var, val) in assignments {
        name = name.replace(&format!("{{{}}}", var), &val.variant_name_string());
    }
    if name.contains('{') {
        return Err(syn::Error::new(
            template.span(),
            "invalid metavariable in rename template",
        ));
    }
    Ok(name)
}

impl Parse for SigmaEnum {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let subattrs = Attribute::parse_outer(input)?;
        let visibility: Visibility = input.parse()?;
        let _: Token![enum] = input.parse()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let mut variants = Vec::new();
        let mut variant_tys = BTreeSet::new();
        let mut attrs = Vec::new();
        while !content.is_empty() {
            let mut expand = BTreeMap::new();
            let mut rename = None;
            let mut docs = None;
            if let Ok(attributes) = content.call(Attribute::parse_outer) {
                for attr in &attributes {
                    if attr.path().is_ident("sigma_enum") {
                        attr.parse_nested_meta(|meta| {
                            match meta.path.require_ident()?.to_string().as_str() {
                                "expand" => {
                                    meta.parse_nested_meta(|meta| {
                                        let ident = meta.path.require_ident()?;
                                        let value: Expr = meta.value()?.parse()?;
                                        let value = extract_expansion(&value)?;
                                        if expand.contains_key(ident) {
                                            return Err(syn::Error::new(
                                                meta.path.span(),
                                                "duplicate expand attribute",
                                            ));
                                        }
                                        expand.insert(ident.clone(), value);
                                        Ok(())
                                    })?;
                                }
                                "rename" => {
                                    if rename.is_some() {
                                        return Err(syn::Error::new(
                                            meta.path.span(),
                                            "duplicate rename attribute",
                                        ));
                                    }
                                    let _: Token![=] = meta.input.parse()?;
                                    if let Ok(ident) = meta.input.parse::<Ident>() {
                                        rename = Some(ident.to_string());
                                    } else if let Ok(template) = meta.input.parse::<LitStr>() {
                                        rename = Some(template.value());
                                    } else {
                                        return Err(syn::Error::new(
                                            meta.input.span(),
                                            "invalid renaming template",
                                        ));
                                    }
                                }
                                "docs" => {
                                    if docs.is_some() {
                                        return Err(syn::Error::new(
                                            meta.path.span(),
                                            "duplicate docs attribute",
                                        ));
                                    }
                                    let _: Token![=] = meta.input.parse()?;
                                    if let Ok(template) = meta.input.parse::<LitStr>() {
                                        docs = Some(template.value());
                                    } else {
                                        return Err(syn::Error::new(
                                            meta.input.span(),
                                            "invalid docstring template",
                                        ));
                                    }
                                }
                                _ => {
                                    return Err(syn::Error::new(meta.path.span(), "invalid attr"));
                                }
                            }
                            Ok(())
                        })?;
                    } else {
                        attrs.push(attr.clone());
                    }
                }
            }

            // variant name
            // we cannot have rename and variant name

            let enum_var_name: Ident = content.parse()?;
            let enum_var_name =
                (!enum_var_name.to_string().starts_with("_")).then_some(enum_var_name);
            if rename.is_some() && enum_var_name.is_some() {
                return Err(syn::Error::new(
                    enum_var_name.span(),
                    "cannot use variant name and rename attribute",
                ));
            }
            if !expand.is_empty() && enum_var_name.is_some() {
                return Err(syn::Error::new(
                    enum_var_name.span(),
                    "cannot use variant name and expand attribute",
                ));
            }

            let ty_paren;
            parenthesized!(ty_paren in content);
            let nice_type: NiceType<Infallible> = ty_paren.parse()?;
            assert!(ty_paren.is_empty());
            let _ = content.parse::<Token![,]>();

            if rename.as_deref().is_some_and(|rename| {
                !expand
                    .keys()
                    .all(|ident| rename.contains(&format!("{{{}}}", ident)))
            }) {
                return Err(syn::Error::new(
                    enum_var_name.span(),
                    "rename template does not have all metavariables",
                ));
            }

            let cartesian: Vec<Vec<(Ident, NiceTypeLit)>> =
                expand
                    .into_iter()
                    .fold(vec![Vec::new()], |accum, (ident, range)| {
                        accum
                            .into_iter()
                            .flat_map(|a| {
                                range.iter().map({
                                    let ident = &ident;
                                    move |r| {
                                        let mut a = a.clone();
                                        a.push((ident.clone(), r.clone()));
                                        a
                                    }
                                })
                            })
                            .collect()
                    });

            for assignments in cartesian {
                let mut var_type = nice_type.clone();
                for (ident, r) in &assignments {
                    var_type = var_type.replace_ident(&ident.to_string(), &r)
                }
                let name = match &rename {
                    Some(template) => {
                        format_ident!("{}", substitute_template(&template, &assignments)?)
                    }
                    None => match &enum_var_name {
                        Some(enum_var_name) => enum_var_name.clone(),
                        None => var_type.variant_name(),
                    },
                };
                let docs = match &docs {
                    Some(template) => {
                        let docstring = substitute_template(&template, &assignments)?;
                        quote! {#[doc = #docstring]}
                    }
                    None => quote! {},
                };
                if !variant_tys.insert(var_type.clone()) {
                    return Err(syn::Error::new(var_type.span(), "duplicate variant types"));
                }
                variants.push(Variant {
                    ty: var_type,
                    name,
                    docs,
                    attrs: attrs.clone(),
                });
            }
        }

        Ok(SigmaEnum {
            visibility,
            name,
            variants,
            subattrs,
            attr: ItemAttr::default(),
        })
    }
}

#[proc_macro_attribute]
pub fn sigma_enum(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let mut sigma_enum = parse_macro_input!(item as SigmaEnum);
    let attr = parse_macro_input!(attr as ItemAttr);
    sigma_enum.attr = attr;

    // panic!("{}", quote! { #sigma_enum });
    quote! { #sigma_enum }.into()
}
