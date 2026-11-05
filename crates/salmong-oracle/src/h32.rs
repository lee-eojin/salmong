pub fn h32(s: u32, i: u32) -> u32 {
    let mut x = s ^ i.wrapping_mul(0x9E3779B1);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EBCA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2AE35);
    x ^= x >> 16;
    x
}

pub fn pick(lo: u32, hi: u32, v: u32) -> u32 {
    lo + v % (hi - lo + 1)
}

pub fn r(seed: u32, p: u32, k: u32, a: u32) -> u32 {
    h32(seed, p << 24 | k << 8 | a)
}
