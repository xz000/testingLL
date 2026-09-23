//! Steam AppID 与「版本能力」开关（**编译期**决定，不靠文件夹里的 `steam_appid.txt`）。
//!
//! 同一份源码编出两个产物：
//! - **正式版**（默认，AppID `908660`）：工坊 / 成就 / 天梯 / 云全部可用；
//! - **demo**（`--features client/demo`，AppID `1042120`）：玩法与正式版一致（训练场 + PvP + Steam 大厅），
//!   但**创意工坊 / 成就 / 天梯不提供**（demo app 在 Steamworks 后台也没配这些），云退化为纯本地。
//!
//! 为什么硬编码而不是读 `steam_appid.txt`：
//! - 我们走 `Client::init_app(appid)`，把 AppID 显式传给 Steam flat API，不依赖该文件；
//! - `steam_appid.txt` 只在**开发时脱离 Steam 直跑 exe** 有用（`run-steam.ps1` 会 stage 一份），
//!   发布包不带它。注意：直跑时 Steam 覆盖层可能不挂（覆盖层靠 Steam 判定“当前跑的是哪个 app”），
//!   由 Steam 启动则正常。
//!
//! Steam 大厅天然按 AppID 隔离 → demo 玩家只能与 demo 玩家匹配，**与正式版天然不互通**，无需额外代码。

/// 正式版 AppID（Circle Brawl）。
pub const APP_ID_FULL: u32 = 908_660;

/// demo 版 AppID（Circle Brawl Demo）。
pub const APP_ID_DEMO: u32 = 1_042_120;

/// 是否为 demo 构建（编译期）。
pub const IS_DEMO: bool = cfg!(feature = "demo");

/// 本构建使用的 Steam AppID。
pub const APP_ID: u32 = if IS_DEMO { APP_ID_DEMO } else { APP_ID_FULL };

/// 本构建的创意工坊内容目录名（`steamapps/workshop/content/<APP_ID>`）。
pub fn app_id_str() -> String {
    APP_ID.to_string()
}

/// 本 app 的创意工坊主页（demo 下没有工坊内容，但仍指向自己的页面，避免误导到正式版）。
#[cfg_attr(not(feature = "steam"), allow(dead_code))]
pub fn workshop_url() -> String {
    format!("https://steamcommunity.com/app/{APP_ID}/workshop/")
}

/// 创意工坊功能是否可用（demo 不提供）。
pub fn workshop_enabled() -> bool {
    !IS_DEMO
}

/// 成就 / 统计 / 天梯是否可用（demo 不提供）。
#[cfg_attr(not(feature = "steam"), allow(dead_code))]
pub fn stats_enabled() -> bool {
    !IS_DEMO
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_follows_feature_flag() {
        // 两个构建下各自成立；这条测试同时钉住“改了 feature 也要改常量”的意图。
        if IS_DEMO {
            assert_eq!(APP_ID, APP_ID_DEMO);
            assert_eq!(APP_ID, 1_042_120);
            assert!(!workshop_enabled() && !stats_enabled(), "demo 不提供工坊/统计");
        } else {
            assert_eq!(APP_ID, APP_ID_FULL);
            assert_eq!(APP_ID, 908_660);
            assert!(workshop_enabled() && stats_enabled());
        }
        assert_eq!(app_id_str(), APP_ID.to_string());
        assert!(workshop_url().contains(&app_id_str()), "工坊链接必须指向本 app");
    }
}
