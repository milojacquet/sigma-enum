use sigma_enum::sigma_enum;

struct A;
struct B;

#[sigma_enum]
pub enum AbEnum {
    __(A),
    __(B),
}

struct Foo<T>(T);
impl Foo<A> {
    fn is_a(&self) -> bool {
        true
    }
}
impl Foo<B> {
    fn is_a(&self) -> bool {
        false
    }
}

#[sigma_enum]
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
enum NuEnum {
    #[sigma_enum(expand(N = 0..=3))]
    __(Nu<Mu<N>>),
    __(Nu<Mu<5>>),
    #[sigma_enum(expand(N = [7..9, 11]))]
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

#[test]
fn match_ab_enum() {
    assert_eq!(
        ab_enum_match!(match AbEnum::A(A) {
            A(_ab) => 1,
            B(_ab) => 2,
        }),
        1
    );

    assert_eq!(
        ab_enum_match!(match AbEnum::A(A) {
            B(_ab) => 1,
            _ab => 2,
        }),
        2
    );
}

#[test]
fn match_foo_enum() {
    assert_eq!(
        foo_enum_match!(match FooEnum::Foo_B(Foo(B)) {
            Foo::<A>(_foo) => 1,
            Foo::<B>(_foo) => 2,
        }),
        2
    );

    assert!(foo_enum_match!(match FooEnum::Foo_A(Foo(A)) {
        Foo::<?T>(foo) => foo.is_a(),
        Foo::<A>(_foo) => false, // intentionally does not match
    }),);

    assert!(foo_enum_match!(match FooEnum::Foo_A(Foo(A)) {
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

    // check const in there. compile fail
    // assert_eq!(
    //     nu_enum_match!(match (NuEnum::Nu_Mu_2(Nu(Mu([(), ()])))) {
    //         Nu::<Mu<?N>>(_nu) => {
    //             let _mu_arr = [(); N];
    //             N
    //         }
    //     }),
    //     2
    // );
}
