#![no_implicit_prelude]
#![no_std]
mod tests {
    use ::sigma_enum::sigma_enum;

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
        #![no_implicit_prelude]
        use super::A;
        use super::B;
        use super::sigma_enum;

        #[sigma_enum(path = crate::tests::inner)]
        pub enum AbEnumI {
            __(A),
            __(B),
        }
    }
}

fn main() {}
