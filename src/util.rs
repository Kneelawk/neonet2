pub fn least_power_of_2_greater(x: u64) -> u64 {
    if x < 1 {
        return 0;
    }

    let mut x = x;
    x -= 1;
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    x |= x >> 32;
    x + 1
}
