use sigma_enum::sigma_enum;

struct Mu<const N: usize>([(); N]);

#[sigma_enum]
enum MuEnum {
    Mu5(Mu<5>),
    MuMu5(Mu<5>),
}

fn main() {}
