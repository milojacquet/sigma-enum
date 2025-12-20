use sigma_type::sigma_type;

struct A;
struct B;

#[sigma_type]
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

#[sigma_type]
enum FooEnum {
    __(Foo<A>),
    __(Foo<B>),
}

struct Nu<const N: usize>();

#[sigma_type]
enum NuEnum {
    #[sigma_type(expand(N = 0..=3))]
    __(Nu<N>),
    __(Nu<5>),
    #[sigma_type(expand(N = [7..9, 11]))]
    __(Nu<N>),
}

#[sigma_type]
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
        Foo::<(?T)>(foo) => foo.is_a(),
        Foo::<A>(_foo) => false, // intentionally does not match
    }),);

    assert!(foo_enum_match!(match FooEnum::Foo_A(Foo(A)) {
        foo => foo.is_a(),
    }),);
}

#[test]
fn match_nu_enum() {
    assert_eq!(
        nu_enum_match!(match (NuEnum::Nu_2(Nu())) {
            Nu::<0>(_foo) => 9,
            Nu::<(?N)>(_foo) => N,
        }),
        2
    );
}
