use crate::scenario::RampPoint;

pub fn rate(ramp: &[RampPoint], t: u64) -> u64 {
    let i = ramp.partition_point(|p| u64::from(p.tick) <= t);
    let a = ramp[i - 1];
    let Some(b) = ramp.get(i) else {
        return u64::from(a.rate);
    };
    let (t0, r0) = (u64::from(a.tick), u64::from(a.rate));
    let (t1, r1) = (u64::from(b.tick), u64::from(b.rate));
    r0 + (r1 - r0) * (t - t0) / (t1 - t0)
}
