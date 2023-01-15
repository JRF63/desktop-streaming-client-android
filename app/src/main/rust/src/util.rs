use ndk_sys::{clock_gettime, timespec, CLOCK_MONOTONIC};
use std::os::raw::c_int;

pub(crate) fn system_nanotime() -> u64 {
    let mut now = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        let _ignored = clock_gettime(CLOCK_MONOTONIC as c_int, &mut now);
    }
    (now.tv_sec as u64)
        .wrapping_mul(1_000_000_000)
        .wrapping_add(now.tv_nsec as u64)
}

/// Compute the greatest common divisor of two numbers.
// https://en.wikipedia.org/wiki/Binary_GCD_algorithm
pub fn gcd(mut u: i32, mut v: i32) -> i32 {
    use std::cmp::min;
    use std::mem::swap;

    if u == 0 {
        return v;
    } else if v == 0 {
        return u;
    }

    let i = u.trailing_zeros();
    u >>= i;
    let j = v.trailing_zeros();
    v >>= j;
    let k = min(i, j);

    loop {
        if u > v {
            swap(&mut u, &mut v);
        }
        v -= u;
        if v == 0 {
            return u << k;
        }
        v >>= v.trailing_zeros();
    }
}