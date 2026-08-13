//! 确定性伪随机数生成器（LCG）。
//!
//! 帧同步模拟需要所有客户端生成相同的随机序列，因此不允许使用系统 RNG。
//! 通过共享的种子与固定的递推公式保证一致性。此处的"种子"由世界状态提供。

/// 简单的 64 位 LCG，适用于游戏模拟中的确定性随机（非加密用途）。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Rng(u64);

const MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;
const INCREMENT: u64 = 0xBF58_476D_1CE4_E5B9;

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Rng(seed)
    }

    /// 推进生成器并返回下一个值。
    #[inline]
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(MULTIPLIER)
            .wrapping_add(INCREMENT);
        self.0
    }

    /// 返回 [0, n) 区间内的整数（n > 0）。
    pub fn next_u64_below(&mut self, n: u64) -> u64 {
        debug_assert!(n > 0);
        self.next() % n
    }

    /// 返回 [0.0, 1.0) 的定点小数。
    pub fn next_fix(&mut self) -> super::fix::Fix64 {
        // 取低 32 位作为 Q32.32 的小数部分
        super::fix::Fix64::from_bits((self.next() >> 32) as i32 as i64)
    }

    /// 返回 [-1.0, 1.0] 的定点小数。
    pub fn next_fix_signed(&mut self) -> super::fix::Fix64 {
        self.next_fix() * super::fix::Fix64::from_num(2) - super::fix::Fix64::ONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_sequence() {
        let mut a = Rng::new(12345);
        let mut b = Rng::new(12345);
        for _ in 0..1000 {
            assert_eq!(a.next(), b.next());
        }
    }

    #[test]
    fn different_seed_differs() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next(), b.next());
    }
}
