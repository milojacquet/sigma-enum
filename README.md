`sigma_enum` is a procedural macro that allows a family of types to be
summed into an enum and pattern matched on. It implements Σ types, also
known as dependent pairs. The macro exports a set of derive macros that
allow runtime values to be lifted to compile time.

## Quick start

```rust
use sigma_enum::sigma_enum;

#[derive(Debug)]
struct Bytes<const N: usize>([u8; N]);

// Define sigma enum
#[sigma_enum(generic(Bytes<usize>))]
enum BytesEnum {
    __(usize), // A standalone type
    #[sigma_enum(expand(N = 0..10))]
    __(Bytes<N>), // Types indexed by a const generic
}

let n: usize = "8".parse().unwrap();
// Construct based on a runtime value
let bytes = bytes_enum_construct!(Bytes::<?n>(Bytes([0x41; n]))).unwrap();

// Match on the const generic in the type
let displayed = bytes_enum_match!(match bytes {
    usize(bytes) => format!("usize: {bytes}"),
    Bytes::<?N>(bytes) => format!("{N} bytes: {bytes:?}"),
});
```

## Basic usage

The most basic use for `sigma_enum` is applying it to an enum of tuple
struct variants, each of which has one value.

```rust
#[sigma_enum]
enum Numeric {
    __(i32),
    __(i64),
}
```

The name of the variants will be automatically generated if they start with
an underscore, and the provided names will be used instead.

Generating an enum that simulates a type that depends on a const generic
value can be done by using the const generic in the type, or in shorthand
with the `expand` attribute. Valid specifications for `expand` metavariables
are literals, ranges, and arrays of those.

In order to use const generics in an enum, the attribute macro should be
annotated with the const generic types used within using the `generic`
attribute. If not, certain functionality will be unavailable. Non-const
generics can be annotated with `_`.

```rust
struct Array<T, const N: usize>([T; N]);

#[sigma_enum]
enum BytesEnum {
    #[sigma_enum(expand(N = 0..3))]
    __(Array<u8, N>),
}

// equivalent to
#[sigma_enum(generic(Array<_ ,usize>))]
enum BytesEnum2 {
    __(Array<u8, 0>),
    __(Array<u8, 1>),
    __(Array<u8, 2>),
}
```

Types used as enum variants for now must only be written with identifiers,
literals, and `<>`.

### Renaming variants

In addition to specifying the name of a variant with the name used in the
enum, renaming can also be done with the `rename` attribute. Used on a
standard variant, it can be used to select a name for the variant. Used on a
variant with the `expand` attribute, a format string can be provided and the
metavariables used will be interpolated into it.

Since the const generic types are used in macros, fully qualified names
should be used.

```rust
struct Array<T, const N: usize>([T; N]);

#[sigma_enum(generic(Array<_, ::std::primitive::usize>))]
enum BytesEnum {
    #[sigma_enum(expand(N = 0..3), rename = "ByteArray{N}")]
    __(Array<u8, N>),
}

// equivalent to
#[sigma_enum(generic(Array<_, ::std::primitive::usize>))]
enum BytesEnum2 {
    ByteArray0(Array<u8, 0>),
    ByteArray1(Array<u8, 1>),
    ByteArray2(Array<u8, 2>),
}
```

### Renaming types

The only types allowed in the enum specifications are those written as a
single identifier. For more complex types, use the `alias` attribute.

```rust
mod inner {
    pub struct Foo;
}

#[sigma_enum(alias(Foo = inner::Foo))]
enum Foo {
    __(Foo),
}
```

## Generated items

### Macros

`sigma_enum` generates several macros for each enum.

The first is the construction macro. This allows for the construction of
values whose types involve const generics even when the values of the const
generics only exist at runtime. Associated to a type `T`, the construction
macro returns a value of type `Option<T>`. Metavariables used in the type
specification must be preceded with `?`.

```rust
struct Bytes<const N: usize>([u8; N]);

#[sigma_enum(generic(Bytes<usize>))]
enum BytesEnum {
    #[sigma_enum(expand(N = 0..3))]
    __(Bytes<N>),
}

let n: usize = 1;
let bytes = bytes_enum_construct!(Bytes::<?n>(Bytes([0x41; n]))).unwrap();
```

Dual to the construction macro is the match macro. This facilitates the use
of the enum as any of its contained types. Metavariables used in the type
specification must be preceded with `?`.

```rust
#[derive(Debug)]
struct Bytes<const N: usize>([u8; N]);

#[sigma_enum(generic(Bytes<usize>))]
enum BytesEnum {
    #[sigma_enum(expand(N = 0..3))]
    __(Bytes<N>),
}

fn displayed(bytes: BytesEnum) -> String {
    bytes_enum_match!(match bytes {
        Bytes::<?N>(bytes) => format!("{N} bytes: {bytes:?}"),
    })
}
```

## Traits

The `sigma_enum` macro also generates a conversion trait for each macro with
three methods: construction, extraction, and owned extraction, each for
types known at compile time. Helper methods on the enum for extraction and
owned extraction are also generated.

```rust
struct Bytes<const N: usize>([u8; N]);

#[sigma_enum(generic(Bytes<usize>))]
enum BytesEnum {
    #[sigma_enum(expand(N = 0..3))]
    __(Bytes<N>),
}

let bytes_enum = Bytes([0x41; 2]).into_bytes_enum(); // uses IntoBytesEnum trait
let bytes: &Bytes<2> = Bytes::<2>::try_from_bytes_enum(&bytes_enum).unwrap(); // uses IntoBytesEnum trait
let bytes: Bytes<2> = bytes_enum.extract_owned().unwrap();
```

These allow the construction and extraction of values with known types.

`From`, `Into`, `TryFrom`, and `TryInto` will also be implemented for the
enum and all types it contains as variants.

## Public API generation

Marking the enum `pub` will export the generated macros and traits as
part of the public API.

For public enums, you must use the `path` attribute to `sigma_enum` with the
absolute path to the current module. This is a limitation of Rust's module
system.

```rust
mod inner {
    use sigma_enum::sigma_enum;
    pub struct Foo;

    #[sigma_enum(path = crate::inner)]
    enum FooEnum {
        __(Foo),
    }
}
```

### Renaming generated items

Generated items can be renamed and docstrings can be provided with the
following attributes:

| Item                               | Attribute name          |
| ---------------------------------- | ----------------------- |
| Construction macro                 | `macro_construct`       |
| Match macro                        | `macro_match`           |
| Enum trait                         | `into_trait`            |
| Enum trait construction method     | `into_method`           |
| Enum trait extraction method       | `try_from_method`       |
| Enum trait owned extraction method | `try_from_owned_method` |
| Enum trait mut extraction method   | `try_from_mut_method`   |
| Enum extraction method             | `extract_method`        |
| Enum owned extraction method       | `extract_owned_method`  |
| Enum mut extraction method         | `extract_mut_method`    |
| Enum trait error                   | `try_from_error`        |

```rust
#[sigma_enum(
    macro_construct(name = make_numeric, docs = "Make a numeric value."),
    macro_match(name = match_numeric, docs = "Match a numeric value.")
)]
enum Numeric {
    __(i32),
    __(i64),
}
```

## Additional information

Derive macros and other item attributes will work when placed below the
`sigma_enum` macro. Variant attributes will be copied to every instance of
the variant if expanded.
