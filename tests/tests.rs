use sigma_type::sigma_type;

struct A;
struct B;
struct Foo<T>(T);

#[sigma_type]
enum AbEnum {
    __(A),
    __(B),
}

// #[sigma_type]
// enum FooEnum {
//     __(Foo<A>),
//     __(Foo<B>),
// }

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
            T(_ab) => 2,
        }),
        2
    );
}

// #[test]
// fn match_foo_enum() {
//     assert_eq!(
//         foo_enum_match!(match FooEnum::Foo_A(A) {
//             A(foo) => 1,
//             B(foo) => 2,
//         }),
//         1
//     );
// }
