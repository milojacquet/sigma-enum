use sigma_enum::sigma_enum;

struct Mu<const N: usize>([(); N]);

#[sigma_enum]
enum MuEnum {
    #[sigma_enum(expand(N = 0..5))]
    __(Mu<5>),
}

fn main() {}
