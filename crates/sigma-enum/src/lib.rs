//! `sigma_enum` is a procedural macro that allows a family of types to be
//! summed into an enum and pattern matched on. It implements Σ types, also
//! known as dependent pairs. The macro exports a set of derive macros that
//! allow runtime values to be lifted to compile time.
//!
//! ## Quick start
//!
//! ```rust
//! use sigma_enum::sigma_enum;
//!
//! #[derive(Debug)]
//! struct Bytes<const N: usize>([u8; N]);
//!
//! // Define sigma enum
//! #[sigma_enum(generic(Bytes<usize>))]
//! enum BytesEnum {
//!     __(usize), // A standalone type
//!     #[sigma_enum(expand(N = 0..10))]
//!     __(Bytes<N>), // Types indexed by a const generic
//! }
//!
//! let n: usize = "8".parse().unwrap();
//! // Construct based on a runtime value
//! let bytes = bytes_enum_construct!(Bytes::<?n>(Bytes([0x41; n]))).unwrap();
//!
//! // Match on the const generic in the type
//! let displayed = bytes_enum_match!(match bytes {
//!     usize(bytes) => format!("usize: {bytes}"),
//!     Bytes::<?N>(bytes) => format!("{N} bytes: {bytes:?}"),
//! });
//! ```
//!
//! ## Basic usage
//!
//! The most basic use for `sigma_enum` is applying it to an enum of tuple
//! struct variants, each of which has one value.
//!
//! ```rust
//! # use sigma_enum::sigma_enum;
//! #[sigma_enum]
//! enum Numeric {
//!     __(i32),
//!     __(i64),
//! }
//! ```
//!
//! The names of the variants will be automatically generated if the provided
//! names start with an underscore, and the provided names will be used
//! otherwise.
//!
//! Generating an enum that simulates a type that depends on a const generic
//! value can be done by using the const generic in the type, or in shorthand
//! with the `expand` attribute. Valid specifications for `expand` metavariables
//! are literals, ranges, and arrays of those.
//!
//! In order to use const generics in an enum, the attribute macro should be
//! annotated with the const generic types used within using the `generic`
//! attribute. If not, certain functionality will be unavailable. Non-const
//! generics can be annotated with `_`.
//! Since the const generic types are used in declarative macros, fully
//! qualified names should be used.
//!
//! ```rust
//! # use sigma_enum::sigma_enum;
//! struct Array<T, const N: usize>([T; N]);
//!
//! #[sigma_enum(generic(Array<_, ::std::primitive::usize>))]
//! enum BytesEnum {
//!     #[sigma_enum(expand(N = 0..3))]
//!     __(Array<u8, N>),
//! }
//!
//! // equivalent to
//! #[sigma_enum(generic(Array<_, ::std::primitive::usize>))]
//! enum BytesEnum2 {
//!     __(Array<u8, 0>),
//!     __(Array<u8, 1>),
//!     __(Array<u8, 2>),
//! }
//! ```
//!
//! Types used as enum variants for now must only be written with identifiers,
//! literals, and `<>`.
//!
//! ### Renaming variants
//!
//! In addition to specifying the name of a variant with the name used in the
//! enum, renaming can also be done with the `rename` attribute. Used on a
//! standard variant, it can be used to select a name for the variant. Used on a
//! variant with the `expand` attribute, a format string can be provided and the
//! metavariables used will be interpolated into it.
//!
//! ```rust
//! # use sigma_enum::sigma_enum;
//! struct Array<T, const N: usize>([T; N]);
//!
//! #[sigma_enum(generic(Array<_, ::std::primitive::usize>))]
//! enum BytesEnum {
//!     #[sigma_enum(expand(N = 0..3), rename = "ByteArray{N}")]
//!     __(Array<u8, N>),
//! }
//!
//! // equivalent to
//! #[sigma_enum(generic(Array<_, ::std::primitive::usize>))]
//! enum BytesEnum2 {
//!     ByteArray0(Array<u8, 0>),
//!     ByteArray1(Array<u8, 1>),
//!     ByteArray2(Array<u8, 2>),
//! }
//! ```
//!
//! ### Renaming types
//!
//! The only types allowed in variant specifications are those written as
//! unqualified identifiers, optionally with generic parameters. For qualified
//! type names, use the `alias` attribute.
//!
//! ```rust
//! # use sigma_enum::sigma_enum;
//! mod inner {
//!     pub struct Foo;
//! }
//!
//! #[sigma_enum(alias(Foo = inner::Foo))]
//! enum Foo {
//!     __(Foo),
//! }
//! ```
//!
//!
//! ## Generated items
//!
//! ### Macros
//!
//! `sigma_enum` generates several macros for each enum.
//!
//! The first is the construction macro. This allows for the construction of
//! values whose types involve const generics even when the values of the const
//! generics only exist at runtime. Associated to a type `T`, the construction
//! macro returns a value of type `Option<T>`. Metavariables used in the type
//! specification must be preceded with `?`.
//!
//! ```rust
//! # use sigma_enum::sigma_enum;
//! struct Bytes<const N: usize>([u8; N]);
//!
//! #[sigma_enum(generic(Bytes<usize>))]
//! enum BytesEnum {
//!     #[sigma_enum(expand(N = 0..3))]
//!     __(Bytes<N>),
//! }
//!
//! let n: usize = 1;
//! let bytes = bytes_enum_construct!(Bytes::<?n>(Bytes([0x41; n]))).unwrap();
//! ```
//!
//! Dual to the construction macro is the match macro. This facilitates the use
//! of the enum as any of its contained types. Metavariables used in the type
//! specification must be preceded with `?`.
//!
//! ```rust
//! # use sigma_enum::sigma_enum;
//! #[derive(Debug)]
//! struct Bytes<const N: usize>([u8; N]);
//!
//! #[sigma_enum(generic(Bytes<usize>))]
//! enum BytesEnum {
//!     #[sigma_enum(expand(N = 0..3))]
//!     __(Bytes<N>),
//! }
//!
//! fn displayed(bytes: BytesEnum) -> String {
//!     bytes_enum_match!(match bytes {
//!         Bytes::<?N>(bytes) => format!("{N} bytes: {bytes:?}"),
//!     })
//! }
//! ```
//!
//! ## Traits
//!
//! The `sigma_enum` macro also generates a conversion trait for each enum with
//! methods for constructing values of the enum of a known variant and
//! extracting a value of a known type from the enum. Helper methods on the enum
//! for extraction are also generated.
//!
//! ```rust
//! # use sigma_enum::sigma_enum;
//! struct Bytes<const N: usize>([u8; N]);
//!
//! #[sigma_enum(generic(Bytes<usize>))]
//! enum BytesEnum {
//!     #[sigma_enum(expand(N = 0..3))]
//!     __(Bytes<N>),
//! }
//!
//! let bytes_enum = Bytes([0x41; 2]).into_bytes_enum(); // uses IntoBytesEnum trait
//! let bytes: &Bytes<2> = Bytes::<2>::try_from_bytes_enum(&bytes_enum).unwrap(); // uses IntoBytesEnum trait
//! let bytes: Bytes<2> = bytes_enum.extract_owned().unwrap();
//! ```
//!
//! `From`, `Into`, `TryFrom`, and `TryInto` will also be implemented between
//! the enum and all types it contains as variants.
//!
//! ## Public API generation
//!
//! Marking the enum `pub` will export the generated macros and traits as
//! part of the public API.
//!
//! <div class="warning">
//!
//! For enums whose macros you intend to use in
//! another module of the crate, you must add the `path` attribute that contains
//! the absolute path to the module.
//!
//! For public enums, you may not use the generated macros in the same crate
//! they were defined in. This is a limitation of Rust's macro system
//! (cf. [issue #52234](https://github.com/rust-lang/rust/pull/52234)).
//! In this case, adding the `path` attribute will generate two sets of macros:
//! one that is exported at the crate root, and another usable in the definition
//! crate whose names are suffixed by `_crate`.
//!
//! ```rust
//! pub mod inner {
//!     # use sigma_enum::sigma_enum;
//!     pub struct Foo;
//!
//!     #[sigma_enum(path = crate::inner)]
//!     pub enum FooEnum {
//!         __(Foo),
//!     }
//! }
//!
//! # fn main(){
//! inner::foo_enum_construct_crate!(Foo(inner::Foo));
//! // foo_enum_construct!(Foo(inner::Foo)); // cannot refer to this macro
//! # }
//! ```
//!
//! </div>
//!
//! ### Renaming generated items
//!
//! Generated items can be renamed and docstrings can be provided with the
//! following attributes:
//!
//! | Item                               | Attribute name          |
//! | ---------------------------------- | ----------------------- |
//! | Construction macro                 | `macro_construct`       |
//! | Match macro                        | `macro_match`           |
//! | Enum trait                         | `into_trait`            |
//! | Enum trait construction method     | `into_method`           |
//! | Enum trait extraction method       | `try_from_method`       |
//! | Enum trait owned extraction method | `try_from_owned_method` |
//! | Enum trait mut extraction method   | `try_from_mut_method`   |
//! | Enum extraction method             | `extract_method`        |
//! | Enum owned extraction method       | `extract_owned_method`  |
//! | Enum mut extraction method         | `extract_mut_method`    |
//! | Enum trait error                   | `try_from_error`        |
//!
//! ```rust
//! # use sigma_enum::sigma_enum;
//! #[sigma_enum(
//!     macro_construct(name = make_numeric, docs = "Make a numeric value."),
//!     macro_match(name = match_numeric, docs = "Match a numeric value.")
//! )]
//! enum Numeric {
//!     __(i32),
//!     __(i64),
//! }
//! ```
//!
//! ## Additional information
//!
//! Derive macros and other item attributes will work when placed below the
//! `sigma_enum` macro. Variant attributes will be copied to every instance of
//! the variant if expanded.

/// The macro.
///
/// See the crate root for detailed documentation.
pub use sigma_enum_macros::sigma_enum;

#[cfg(test)]
mod tests {
    use super::sigma_enum;

    pub struct A;
    pub struct B;

    #[sigma_enum(path = crate::tests)]
    pub enum AbEnum {
        __(A),
        __(B),
    }

    struct FooType<T>(T);
    impl FooType<A> {
        fn is_a(&self) -> bool {
            true
        }
    }
    impl FooType<B> {
        fn is_a(&self) -> bool {
            false
        }
    }

    #[sigma_enum(alias(Foo = FooType))]
    enum FooEnum {
        __(Foo<A>),
        __(Foo<B>),
    }

    #[derive(Debug, Clone, Copy)]
    struct Mu<const N: usize>([(); N]);
    #[derive(Debug, Clone, Copy)]
    struct Nu<M>(M);

    #[sigma_enum(
        generic(Mu<usize>),
        macro_match(name = nu_match, docs =
"A macro to match Nu."
        ),
    )]
    #[derive(Debug, Clone, Copy)]
    #[non_exhaustive]
    enum NuEnum {
        #[sigma_enum(expand(N = 0..=3))]
        __(Nu<Mu<N>>),
        NuMu5(Nu<Mu<5>>),
        #[sigma_enum(expand(N = [7..9, 11]), rename = "NuMu{N}_Big")]
        __(Nu<Mu<N>>),
    }

    #[sigma_enum]
    enum NuEnumNoGen {
        #[sigma_enum(expand(N = 0..=3))]
        __(Nu<Mu<N>>),
        __(Nu<Mu<5>>),
        #[sigma_enum(expand(N = [7..9, 11]))]
        __(Nu<Mu<N>>),
    }

    #[sigma_enum]
    enum EmptyEnum {}

    // test if those in inner modules compile outside
    pub mod inner {
        use crate::sigma_enum;
        use crate::tests::A;
        use crate::tests::B;

        #[sigma_enum(path = crate::tests::inner)]
        pub enum AbEnumI {
            __(A),
            __(B),
        }
    }

    #[test]
    fn match_ab_enum() {
        assert_eq!(
            ab_enum_match_crate!(match ab_enum_construct_crate!(A(A)).unwrap() {
                A(_ab) => 1,
                B(_ab) => 2,
            }),
            1
        );

        assert_eq!(
            ab_enum_match_crate!(match ab_enum_construct_crate!(A(A)).unwrap() {
                B(_ab) => 1,
                _ab => 2,
            }),
            2
        );
    }

    #[test]
    fn match_ab_enum_i() {
        assert_eq!(
            inner::ab_enum_i_match_crate!(match inner::AbEnumI::A(A) {
                A(_ab) => 1,
                B(_ab) => 2,
            }),
            1
        );

        assert_eq!(
            inner::ab_enum_i_match_crate!(match inner::AbEnumI::A(A) {
                B(_ab) => 1,
                _ab => 2,
            }),
            2
        );
    }

    #[test]
    fn match_foo_enum() {
        assert_eq!(
            foo_enum_match!(match FooEnum::Foo_B(FooType(B)) {
                Foo::<A>(_foo) => 1,
                Foo::<B>(_foo) => 2,
            }),
            2
        );

        assert!(foo_enum_match!(match FooEnum::Foo_A(FooType(A)) {
            Foo::<?T>(foo) => foo.is_a(),
            Foo::<A>(_foo) => false, // intentionally does not match
        }),);

        assert!(foo_enum_match!(match FooEnum::Foo_A(FooType(A)) {
            foo => foo.is_a(),
        }),);
    }

    #[test]
    fn match_nu_enum() {
        // let numu2 = NuEnum::Nu_Mu_2(Nu(Mu([(); 2])));
        let n = 2;
        let numu2 = nu_enum_construct!(Nu::<Mu<?n>>({
            let arr = [(); n];
            Nu(Mu(arr))
        }))
        .unwrap();

        assert_eq!(
            nu_match!(match numu2 {
                Nu::<Mu<0>>(_nu) => 9,
                Nu::<Mu<?N>>(_nu) => N,
                _nu => 99,
            }),
            2
        );

        // check const in there
        assert_eq!(
            nu_match!(match numu2 {
                Nu::<Mu<?N>>(_nu) => {
                    let _mu_arr = [(); N];
                    N
                }
            }),
            2
        );

        // make sure types don't blow up the let
        assert_eq!(
            nu_match!(match numu2 {
                Nu::<?T>(_nu) => 2,
            }),
            2
        );
    }

    #[test]
    fn match_nu_enum_no_gen() {
        assert_eq!(
            nu_enum_no_gen_match!(match (NuEnumNoGen::Nu_Mu_2(Nu(Mu([(), ()])))) {
                Nu::<Mu<0>>(_nu) => 9,
                Nu::<Mu<?N>>(_nu) => N,
            }),
            2
        );
    }
}
