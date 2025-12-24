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
struct SigmaType {
    visibility: Visibility,
    name: Ident,
    variants: Vec<(NiceType<Infallible>, Ident)>,
    subattrs: Vec<Attribute>,
    attr: ItemAttr,
}

impl SigmaType {
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

    fn macro_match_docstring(&self) -> proc_macro2::TokenStream {
        self.attr
            .macro_match
            .docs
            .as_ref()
            .map_or_else(|| quote! {}, |docs| quote! { #[doc = #docs] })
    }

    fn macro_construct_docstring(&self) -> proc_macro2::TokenStream {
        self.attr
            .macro_construct
            .docs
            .as_ref()
            .map_or_else(|| quote! {}, |docs| quote! { #[doc = #docs] })
    }

    fn into_trait_docstring(&self) -> proc_macro2::TokenStream {
        self.attr
            .into_trait
            .docs
            .as_ref()
            .map_or_else(|| quote! {}, |docs| quote! { #[doc = #docs] })
    }

    fn into_method_docstring(&self) -> proc_macro2::TokenStream {
        self.attr
            .into_method
            .docs
            .as_ref()
            .map_or_else(|| quote! {}, |docs| quote! { #[doc = #docs] })
    }

    fn internal_name(&self, which: &str) -> Ident {
        format_ident!(
            "{INTERNAL_IDENT}_{}_{}",
            self.name.to_string().to_snake_case(),
            which
        )
    }
}

impl ToTokens for SigmaType {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let SigmaType {
            visibility,
            name,
            variants,
            subattrs,
            attr: _,
        } = &self;
        let variants_btree: BTreeMap<_, _> = variants.iter().cloned().collect();
        let variant_types: Vec<_> = variants.iter().map(|(var, _)| var).collect();
        let variant_names: Vec<_> = variants.iter().map(|(_, name)| name).collect();

        let macro_match = self.macro_match_name();
        let macro_construct = self.macro_construct_name();
        let macro_match_body = self.internal_name("body");
        let macro_match_process_body = self.internal_name("process_body");
        let macro_process_type = self.internal_name("process_type");
        let macro_match_variant = self.internal_name("variant");
        let macro_match_pattern = self.internal_name("pattern");
        let macro_construct_inner = self.internal_name("construct_inner");

        let into_trait = self.into_trait_name();
        let into_trait_sealed_mod = self.internal_name("into_trait_sealed_mod");
        let into_method = self.into_method_name();

        let macro_match_docstring = self.macro_match_docstring();
        let macro_construct_docstring = self.macro_construct_docstring();
        let into_trait_docstring = self.into_trait_docstring();
        let into_method_docstring = self.into_method_docstring();

        let macro_use = match self.visibility {
            Visibility::Public(_) => quote! { #[macro_use] },
            _ => quote! {},
        };

        let internal_full_wildcard = format_ident!("{INTERNAL_FULL_WILDCARD}");

        let mut patterns_map = BTreeMap::new();
        patterns_map.insert(NiceType::PatternIdent(()), Vec::new());
        for ty in &variant_types {
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
                            .map(|q| quote! { #[allow(nonstandard_style)] #q })
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
            #(#subattrs)*
            #visibility enum #name {
                #(#variant_names(#variant_types),)*
            }
        });

        tokens.append_all(quote! {
            #macro_use
            #[allow(unused_macros)]
            #macro_match_docstring
            macro_rules! #macro_match {
                ( match $( $rest:tt )* ) => {
                    #macro_match_body ! { (), ( $($rest)* ) }
                };
            }
        });

        tokens.append_all(quote! {
            #macro_use
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
            #macro_use
            #[doc(hidden)]
            macro_rules! #macro_match_process_body {
                ( $what:tt, (), ( $( ( $ty:tt; $binding:pat => $body:expr ) )* ) ) => {
                    {
                        let what = $what;

                        #[allow(unreachable_patterns)]
                        match what {
                            $( #macro_match_pattern !($ty) => (), )*
                        }

                        #[allow(unused_labels)]
                        'ma: {
                            $( #macro_match_variant !{$ty; what; 'ma; $binding => $body} )*
                            unreachable!();
                        }
                    }
                };
                (
                    $what:tt,
                    ( $binding:ident => { $( $body:tt )* } $(,)? $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( (#internal_full_wildcard) ; $binding => $body ) ) )
                };
                (
                    $what:tt,
                    ( $binding:ident => $body:expr, $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( (#internal_full_wildcard) ; $binding => { $body } ) ) )
                };
                (
                    $what:tt,
                    ( $tyn:ident ( $binding:pat ) => { $( $body:tt )* } $(,)? $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( ($tyn); $binding => { $($body)* } ) ) )
                };
                (
                    $what:tt,
                    ( $tyn:ident ( $binding:pat ) => $body:expr, $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( ($tyn); $binding => { $body } ) ) )
                };
                (
                    $what:tt,
                    ( $tyn:ident ::< $( $rest:tt )* ),
                    ( $( $matched:tt )* )
                ) => {
                    #macro_process_type !( (@match, $what, $tyn, ($( $matched )*)), ($( $rest )*), (<), (<) )
                };
            }
        });

        tokens.append_all(quote! {
            #macro_use
            #[doc(hidden)]
            macro_rules! #macro_process_type {
                ( $bundle:tt, (> $($rest:tt)*), ( $($params:tt)* ), (< $($counter:tt)*) ) => {
                    #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* >), ($($counter)*) )
                };
                ( $bundle:tt, (>> $($rest:tt)*), ( $($params:tt)* ), (< < $($counter:tt)*) ) => {
                    #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* > >), ($($counter)*) )
                };
                ( $bundle:tt, (> $($rest:tt)*), ( $($params:tt)* ), () ) => {
                    compile_error!("imbalanced")
                };
                ( $bundle:tt, (>> $($rest:tt)*), ( $($params:tt)* ), () ) => {
                    compile_error!("imbalanced")
                };
                ( $bundle:tt, (< $($rest:tt)*), ( $($params:tt)* ), ( $($counter:tt)* ) ) => {
                    #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* <), (< $($counter)*) )
                };
                ( $bundle:tt, (<< $($rest:tt)*), ( $($params:tt)* ), ( $($counter:tt)* ) ) => {
                    #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* < <), (< < $($counter)*) )
                };
                ( (@match, $what:tt, $tyn:ident, ( $($matched:tt)* )), (( $binding:pat ) => { $( $body:tt )* } $(,)? $($rest:tt)*), ( $($params:tt)* ), () ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( ($tyn :: $($params)+); $binding => { $($body)* } ) ) )
                };
                ( (@match, $what:tt, $tyn:ident, ( $($matched:tt)* )), (( $binding:pat ) => $body:expr, $($rest:tt)*), ( $($params:tt)* ), () ) => {
                    #macro_match_process_body !( $what, ( $($rest)* ), ( $($matched)* ( ($tyn :: $($params)+); $binding => { $body } ) ) )
                };
                ( (@construct, $tyn:ident), (( $expr:expr )), ( $($params:tt)+ ), () ) => {
                    #macro_construct_inner !( ($tyn :: $($params)+); ( $expr ) )
                };
                ( $bundle:tt, (( $($any:tt)* ) $($rest:tt)*), ( $($params:tt)* ), ( $($counter:tt)* ) ) => {
                    compile_error!("imbalanced or something")
                };
                ( $bundle:tt, ($thing:tt $($rest:tt)*), ( $($params:tt)* ), ( $($counter:tt)* ) ) => {
                    #macro_process_type ! ( $bundle, ($($rest)*), ($($params)* $thing), ( $($counter)*) )
                };
            }
        });

        tokens.append_all(quote! {
            #macro_use
            #[doc(hidden)]
            macro_rules! #macro_match_variant {
                #( ( (#pat_vars_names #(::< #( #pat_vars_params ),* >)* ); $what:ident; $ma:lifetime; $binding:pat => $body:expr ) => {
                    #( if let #name :: #pat_variant_names ($binding) = $what {
                        #const_let_statements
                        break $ma($body);
                    } )*
                }; )*
            }
        });

        tokens.append_all(quote! {
            #macro_use
            #[doc(hidden)]
            macro_rules! #macro_match_pattern {
                #( ( ( #pat_vars_names #(::< #( #pat_vars_params ),* >)* ) ) => {
                    #( #name :: #pat_variant_names (_) )|*
                }; )*
            }
        });

        tokens.append_all(quote! {
            #macro_use
            #[allow(unused_macros)]
            #macro_construct_docstring
            macro_rules! #macro_construct {
                ( $tyn:ident ::< $($tt:tt)* ) => {
                    #macro_process_type !( (@construct, $tyn), ($($tt)*), (<), (<) )
                };
            }
        });

        tokens.append_all(quote! {
            #macro_use
            #[doc(hidden)]
            macro_rules! #macro_construct_inner {
                #( ( (#pat_vars_names #(::< #( #pat_vars_params ),* >)* ); $body:expr ) => {
                    'ma: {
                        #( if true #(&& #pat_vars_params_eqs)* {
                            #const_let_statements
                            break 'ma Some(#name :: #pat_variant_names($body));
                        } )*
                        None
                    }
                }; )*
            }
        });

        if matches!(visibility, Visibility::Public(_)) {
            tokens.append_all(quote! {
                #into_trait_docstring
                pub trait #into_trait : #into_trait_sealed_mod ::Sealed {
                    #into_method_docstring
                    fn #into_method (self) -> #name;
                }

                mod #into_trait_sealed_mod {
                    pub trait Sealed {}
                }

                #( impl #into_trait_sealed_mod ::Sealed for #variant_types {} )*
            });
        } else {
            tokens.append_all(quote! {
                #visibility trait #into_trait {
                    fn #into_method (self) -> #name;
                }
            });
        }

        tokens.append_all(quote! {
            #(
                impl #into_trait for #variant_types {
                    fn #into_method (self) -> #name {
                        #name :: #variant_names (self)
                    }
                }

                impl From<#variant_types> for #name {
                    fn from(value: #variant_types) -> Self {
                        value. #into_method ()
                    }
                }
            )*
        });
    }
}

impl Parse for SigmaType {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let subattrs = Attribute::parse_outer(input)?;
        let visibility: Visibility = input.parse()?;
        let _: Token![enum] = input.parse()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let mut variants = Vec::new();
        while !content.is_empty() {
            let mut expand = BTreeMap::new();
            let mut rename = None;
            if let Ok(attrs) = content.call(Attribute::parse_outer) {
                for attr in attrs {
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
                                                ident.span(),
                                                "duplicate expansion",
                                            ));
                                        }
                                        expand.insert(ident.clone(), value);
                                        Ok(())
                                    })?;
                                }
                                "rename" => {
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
                                _ => {
                                    return Err(syn::Error::new(meta.path.span(), "invalid attr"));
                                }
                            }
                            Ok(())
                        })?;
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
                    content.span(),
                    "cannot use variant name and rename attribute",
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
                        let mut name = template.clone();
                        for (var, val) in assignments {
                            name =
                                name.replace(&format!("{{{}}}", var), &val.variant_name_string());
                        }
                        format_ident!("{}", name)
                    }
                    None => var_type.variant_name(),
                };
                variants.push((var_type, name));
            }
        }

        Ok(SigmaType {
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
    let mut sigma_enum = parse_macro_input!(item as SigmaType);
    let attr = parse_macro_input!(attr as ItemAttr);
    sigma_enum.attr = attr;

    // panic!("{}", quote! { #sigma_enum });
    quote! { #sigma_enum }.into()
}
