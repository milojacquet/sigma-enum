use sigma_enum::sigma_enum;

struct Mu<const N: usize>([(); N]);

#[sigma_enum]
pub(crate) enum MuEnum {
    __(Mu<N>),
}

fn main() {}
