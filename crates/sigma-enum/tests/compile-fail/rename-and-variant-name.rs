use sigma_enum::sigma_enum;

struct Mu<const N: usize>([(); N]);

#[sigma_enum]
enum MuEnum {
    #[sigma_enum(rename = "Mu8")]
    Mu5(Mu<N>),
}

fn main() {}
