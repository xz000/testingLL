//! 定点数数学层。
//!
//! 采用成熟生态：
//! - [`fixed`](https://docs.rs/fixed) 提供 Q31.32 定点算术（纯整数，确定性强、经过大量考验）
//! - [`cordic`](https://docs.rs/cordic) 提供确定性的 CORDIC 三角函数（来自 nalgebra 作者）
//!
//! 这是帧同步确定性的基石：所有客户端在相同输入下由纯整数运算得到完全一致的结果。

pub use cordic::{acos, asin, atan, atan2, cos, sin, sin_cos, tan};

/// 游戏逻辑使用的定点数类型：Q32.32（32 位小数，64 位总宽）。
pub type Fix64 = fixed::types::I32F32;

/// 2D 定点向量。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Vec2 {
    pub x: Fix64,
    pub y: Fix64,
}

impl Vec2 {
    #[inline]
    pub const fn new(x: Fix64, y: Fix64) -> Self {
        Vec2 { x, y }
    }

    pub const ZERO: Vec2 = Vec2 {
        x: Fix64::ZERO,
        y: Fix64::ZERO,
    };

    #[inline]
    pub const fn zero() -> Self {
        Vec2::ZERO
    }

    #[inline]
    pub fn length_squared(self) -> Fix64 {
        self.x * self.x + self.y * self.y
    }

    #[inline]
    pub fn length(self) -> Fix64 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn dot(self, other: Vec2) -> Fix64 {
        self.x * other.x + self.y * other.y
    }

    /// 归一化（无除法为零时给出零向量）。
    #[inline]
    pub fn normalized(self) -> Vec2 {
        let len = self.length();
        if len == Fix64::ZERO {
            Vec2::zero()
        } else {
            self / len
        }
    }

    /// 顺时针旋转 90°。
    #[inline]
    pub fn perp(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }

    /// 从 (x, y) 构造。
    #[inline]
    pub fn from_coords(x: Fix64, y: Fix64) -> Self {
        Vec2::new(x, y)
    }
}

impl core::ops::Add for Vec2 {
    type Output = Vec2;
    #[inline]
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl core::ops::AddAssign for Vec2 {
    #[inline]
    fn add_assign(&mut self, o: Vec2) {
        *self = *self + o;
    }
}
impl core::ops::Sub for Vec2 {
    type Output = Vec2;
    #[inline]
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl core::ops::SubAssign for Vec2 {
    #[inline]
    fn sub_assign(&mut self, o: Vec2) {
        *self = *self - o;
    }
}
impl core::ops::Neg for Vec2 {
    type Output = Vec2;
    #[inline]
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}
impl core::ops::Mul<Fix64> for Vec2 {
    type Output = Vec2;
    #[inline]
    fn mul(self, s: Fix64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}
impl core::ops::Mul<Vec2> for Fix64 {
    type Output = Vec2;
    #[inline]
    fn mul(self, v: Vec2) -> Vec2 {
        v * self
    }
}
impl core::ops::Div<Fix64> for Vec2 {
    type Output = Vec2;
    #[inline]
    fn div(self, s: Fix64) -> Vec2 {
        Vec2::new(self.x / s, self.y / s)
    }
}

/// 逆时针旋转向量。
///
/// 输入旋转角度（弧度），逆时针方向。该函数用于均匀布点等确定性场景。
#[inline]
pub fn rotate_ccw(v: Vec2, angle: Fix64) -> Vec2 {
    let c = cos(angle);
    let s = sin(angle);
    Vec2::new(v.x * c + v.y * s, v.y * c - v.x * s)
}

/// 按法线 `normal` 镜向反射 `origin`（原版 `MirrorBy`）。
///
/// 用法：护盾反弹、回旋镖撞墙。`origin` 是被反射的速度/方向，`normal` 是表面法线（会被归一化）。
#[inline]
pub fn mirror_by(origin: Vec2, normal: Vec2) -> Vec2 {
    let mn = normal.normalized();
    let pl = origin.dot(mn);
    mn * (pl * Fix64::from_num(2)) - origin
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Fix64, b: f64, tol: f64) -> bool {
        (a.to_num::<f64>() - b).abs() < tol
    }
    fn approxv(a: Vec2, x: f64, y: f64, tol: f64) -> bool {
        approx(a.x, x, tol) && approx(a.y, y, tol)
    }

    #[test]
    fn arithmetic() {
        let a = Fix64::from_num(3.5f64);
        let b = Fix64::from_num(2.0f64);
        assert!(approx(a + b, 5.5, 1e-5));
        assert!(approx(a - b, 1.5, 1e-5));
        assert!(approx(a * b, 7.0, 1e-4));
        assert!(approx(a / b, 1.75, 1e-5));
        assert!(approx((a + b).sqrt(), 2.345208, 1e-4));
    }

    #[test]
    fn trig() {
        assert!(approx(sin(Fix64::ZERO), 0.0, 1e-4));
        assert!(approx(sin(Fix64::FRAC_PI_2), 1.0, 1e-4));
        assert!(approx(cos(Fix64::ZERO), 1.0, 1e-4));
        assert!(approx(cos(Fix64::PI), -1.0, 1e-4));
    }

    #[test]
    fn atan2_quadrants() {
        let pi2 = std::f64::consts::FRAC_PI_2;
        assert!(approx(atan2(Fix64::ZERO, Fix64::ONE), 0.0, 1e-3));
        assert!(approx(atan2(Fix64::ONE, Fix64::ZERO), pi2, 1e-3));
        assert!(approx(atan2(Fix64::ONE, Fix64::ONE), std::f64::consts::FRAC_PI_4, 1e-3));
        assert!(approx(atan2(-Fix64::ONE, Fix64::ONE), -std::f64::consts::FRAC_PI_4, 1e-3));
        assert!(approx(
            atan2(Fix64::ONE, -Fix64::ONE),
            3.0 * std::f64::consts::FRAC_PI_4,
            1e-3
        ));
        assert!(approx(
            atan2(-Fix64::ONE, -Fix64::ONE),
            -3.0 * std::f64::consts::FRAC_PI_4,
            1e-3
        ));
    }

    #[test]
    fn vec2_ops() {
        let u = Vec2::new(Fix64::from_num(0.0), Fix64::from_num(3.0));
        let v = Vec2::new(Fix64::from_num(4.0), Fix64::from_num(0.0));
        assert!(approxv(u + v, 4.0, 3.0, 1e-5));
        assert!(approx(u.length(), 3.0, 1e-4));
        assert!(approxv(u.normalized(), 0.0, 1.0, 1e-4));
        assert!(approx(u.dot(v), 0.0, 1e-5));
    }

    #[test]
    fn rotate_ccw_is_deterministic() {
        let v = Vec2::new(Fix64::from_num(1.0), Fix64::ZERO);
        let angle = Fix64::from_num(1.2345678f64);
        let mut prev: Option<Vec2> = None;
        for _ in 0..50 {
            let r = rotate_ccw(v, angle);
            if let Some(p) = prev {
                assert_eq!(p, r);
            }
            prev = Some(r);
        }
    }
}
