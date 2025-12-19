use sigma_enum::sigma_type;

struct A;
struct B;
struct Foo<T>(T);

#[sigma_type]
enum FooEnum {
    Foo_A(Foo<A>),
    Foo_B(Foo<B>),
}

#[test]
fn match_foo_enum() {}
