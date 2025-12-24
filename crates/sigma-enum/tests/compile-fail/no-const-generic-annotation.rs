use sigma_enum::sigma_enum;

struct Mu<const N: usize>([(); N]);

#[sigma_enum]
enum MuEnum {
    Mu5(Mu<5>),
}

fn main() {
    mu_enum_match!(match (MuEnum::Mu5(Mu([(); 5]))) {
        Mu::<?N>(_mu) => {
            let _mu_arr = [(); N];
        }
    })
}
