//! 客户端本地特效系统（表现层 P3）。
//!
//! **纯客户端、不进快照、不影响确定性**（[`PRESENTATION_PLAN.md`] §2/§6）：只接收"世界坐标 + 类型 + 时长"，
//! 自己推进生命周期并绘制。事件源由 `Game::update_presentation`（hp 差分 / 战斗事件）提供。
//!
//! 位置与半径用 **世界单位 f32**；绘制时经 `offset/scale` 换算到屏幕（与玩家/飘字一致）。
//! 颜色用 `[f32; 4]`（避免在纯逻辑里依赖 ggez 类型，便于单测）。

use ggez::graphics::{Canvas, Color, DrawMode, DrawParam, Mesh};
use ggez::mint::Point2;
use ggez::{Context, GameResult};

/// 特效种类。目前仅 P3-1（命中视觉）；P3-2/P3-3 再扩展 `Ring`/`Debris`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxKind {
    /// 命中闪光：目标外向扩散的圆环（受击反馈）。
    HitFlash,
    /// 命中火花：从命中点散开的实心小点。
    Spark,
}

/// 一个特效实例。`life`/`max_life` 决定进度；`radius` 为世界单位基准半径。
#[derive(Clone, Copy, Debug)]
pub struct Fx {
    pub kind: FxKind,
    /// 世界坐标 `[x, y]`。
    pub pos: [f32; 2],
    /// `[r, g, b, a]`，各 0–1。
    pub color: [f32; 4],
    pub life: f32,
    pub max_life: f32,
    pub radius: f32,
}

/// 特效上限（防刷屏）：超过则丢最旧的。
pub const FX_MAX: usize = 256;

/// 特效集合。
#[derive(Default)]
pub struct FxSystem {
    items: Vec<Fx>,
}

impl FxSystem {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// 生成一个特效（带上限，超出丢最旧）。
    pub fn spawn(&mut self, fx: Fx) {
        if self.items.len() >= FX_MAX {
            self.items.remove(0);
        }
        self.items.push(fx);
    }

    /// 推进生命周期（`dt` 秒），移除已消亡者。
    pub fn update(&mut self, dt: f32) {
        self.items.retain_mut(|f| {
            f.life -= dt;
            f.life > 0.0
        });
    }

    /// 在世界层绘制（实体之后、飘字/横幅之前）。`offset/scale` 与玩家绘制一致。
    pub fn draw(
        &self,
        canvas: &mut Canvas,
        ctx: &Context,
        offset: Point2<f32>,
        scale: f32,
    ) -> GameResult {
        for f in &self.items {
            let t = progress(f); // 1（刚生成）→ 0（消亡）
            let a = alpha(f);
            let c = Color::new(f.color[0], f.color[1], f.color[2], f.color[3] * a);
            let x = f.pos[0] * scale + offset.x;
            let y = f.pos[1] * scale + offset.y;
            match f.kind {
                FxKind::HitFlash => {
                    // 半径随进度向外扩散，线宽随进度变细。
                    let r = (f.radius * (0.9 + (1.0 - t) * 0.7) * scale).max(2.0);
                    let w = (1.0 + t * 2.5).max(1.0);
                    let ring = Mesh::new_circle(&ctx.gfx, DrawMode::stroke(w), Point2 { x, y }, r, 0.5, c)?;
                    canvas.draw(&ring, DrawParam::new());
                }
                FxKind::Spark => {
                    // 小点随进度收缩并淡出。
                    let r = (f.radius * t * scale).max(0.5);
                    let dot = Mesh::new_circle(&ctx.gfx, DrawMode::fill(), Point2 { x, y }, r, 0.5, c)?;
                    canvas.draw(&dot, DrawParam::new());
                }
            }
        }
        Ok(())
    }
}

/// 进度：`1.0`（刚生成）→ `0.0`（消亡）。纯函数，便于单测。
pub fn progress(f: &Fx) -> f32 {
    if f.max_life <= 0.0 {
        return 0.0;
    }
    (f.life / f.max_life).clamp(0.0, 1.0)
}

/// 淡出透明度：与进度线性一致。纯函数。
pub fn alpha(f: &Fx) -> f32 {
    progress(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spark(life: f32) -> Fx {
        Fx { kind: FxKind::Spark, pos: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0], life, max_life: life, radius: 6.0 }
    }

    #[test]
    fn fx_fades_and_expires() {
        let mut s = FxSystem::new();
        s.spawn(spark(0.2));
        assert_eq!(s.items.len(), 1);
        assert!((alpha(&s.items[0]) - 1.0).abs() < 1e-6);
        s.update(0.1);
        assert!((alpha(&s.items[0]) - 0.5).abs() < 1e-3);
        s.update(0.2);
        assert!(s.items.is_empty(), "life 归零后应被移除");
    }

    #[test]
    fn fx_caps_and_drops_oldest() {
        let mut s = FxSystem::new();
        for i in 0..(FX_MAX + 5) {
            let mut f = spark(1.0);
            f.kind = FxKind::HitFlash;
            f.pos = [i as f32, 0.0];
            s.spawn(f);
        }
        assert_eq!(s.items.len(), FX_MAX, "超过上限应截断");
        // 最旧的 5 个被丢弃 → 现存第一个是 i=5。
        assert!((s.items[0].pos[0] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn progress_handles_zero_life() {
        let f = Fx { kind: FxKind::Spark, pos: [0.0, 0.0], color: [1.0; 4], life: 0.0, max_life: 0.0, radius: 1.0 };
        assert_eq!(progress(&f), 0.0);
    }
}
