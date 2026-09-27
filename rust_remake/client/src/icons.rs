//! 图标包图像缓存（ggez 侧）：按 [`IconKey`] 懒加载 + 缓存；换包清空。
//!
//! 纯表现层：只读本地文件，**不进** `World`/快照。缺文件/解码失败 → `None`（HUD 回退文字）。

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ggez::graphics::{self, Color, DrawParam, Image};
use ggez::mint::{Point2, Vector2};
use ggez::Context;

use crate::icon_pack::{self, IconKey};

/// 图标图像缓存。`root = None` 表示未选包（HUD 保持纯文字）。
pub struct IconBank {
    root: Option<PathBuf>,
    cache: RefCell<HashMap<IconKey, Option<Image>>>,
}

impl IconBank {
    pub fn new() -> Self {
        Self { root: None, cache: RefCell::new(HashMap::new()) }
    }

    /// 设置/清除当前图标包根（根不变则保留缓存；变了清空）。
    pub fn set_pack(&mut self, root: Option<PathBuf>) {
        if self.root != root {
            self.root = root;
            self.cache.borrow_mut().clear();
        }
    }

    /// 取某键的图标（懒加载 + 缓存）。未选包/无文件/解码失败 → `None`。
    ///
    /// 返回**克隆**（`Image` 是轻量句柄），以便在 `&self` 借用下使用、避开与 HUD 只读状态的借用冲突。
    pub fn icon(&self, ctx: &Context, key: IconKey) -> Option<Image> {
        let root = self.root.as_ref()?;
        let mut cache = self.cache.borrow_mut();
        if let Some(v) = cache.get(&key) {
            return v.clone();
        }
        let img = load(ctx, root, key);
        cache.insert(key, img.clone());
        img
    }
}

impl Default for IconBank {
    fn default() -> Self {
        Self::new()
    }
}

fn load(ctx: &Context, root: &Path, key: IconKey) -> Option<Image> {
    let path = icon_pack::icon_path(root, key)?;
    let bytes = std::fs::read(&path).ok()?;
    match Image::from_bytes(&ctx.gfx, &bytes) {
        Ok(img) => Some(img),
        Err(e) => {
            eprintln!("[icon] 解码失败 {}: {e:?}", path.display());
            None
        }
    }
}

/// 等比缩放居中绘制到 `rect`（不变形）。
pub fn draw_fitted(canvas: &mut graphics::Canvas, img: &Image, rect: graphics::Rect, tint: Color) {
    let iw = img.width() as f32;
    let ih = img.height() as f32;
    if iw <= 0.0 || ih <= 0.0 {
        return;
    }
    let scale = (rect.w / iw).min(rect.h / ih);
    let w = iw * scale;
    let h = ih * scale;
    canvas.draw(
        img,
        DrawParam::new()
            .dest(Point2 {
                x: rect.x + (rect.w - w) * 0.5,
                y: rect.y + (rect.h - h) * 0.5,
            })
            .scale(Vector2 { x: scale, y: scale })
            .color(tint),
    );
}
