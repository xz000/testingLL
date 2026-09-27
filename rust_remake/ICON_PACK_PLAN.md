# 图标包 / 创意工坊方案（2026-09-27）

> 目标：给 **HUD 技能槽** 与 **物品栏 / 商店** 加图标，并允许玩家用**自制图标包**替换。
> 与音频包同构：**纯客户端**，不进 `World`/快照、不影响帧同步；缺素材/解码失败 → **回退到现有文字**。
> 姊妹文档：`AUDIO_PLAN.md` §8（外部音频包 / 创意工坊）——本方案多处直接复用其机制。

---

## 0. 关键结论（先看这个）

- **内置图标默认「无」**：不选任何包时，HUD/商店保持**现状（纯文字）**，零资源也能玩。
  图标是**纯增量**：装了包才有图，缺某个键就只有那个键回退文字。
- 图标**独立成一种资源包类型**（`type=icons`），与音效包/BGM 包分开选择。
- 包来源、扫描、热重载、工坊发布/订阅**全部复用 `audio_pack.rs` 的既有骨架**（本地 > 工坊，去重、Steam 库探测等）。
- **本次不做状态图标**（`active_status_icons`：熔岩护盾 / 守护盾 / 隐身 / 束缚…这批 buff 小图标）。
  那是 `PRESENTATION_PLAN.md` P4-3 的范畴，且当时已决定走「主题色 + 符号」而非外部贴图；本次维持不动。
- 需要一个**示例包**给工坊做演示：由程序用**现有 CJK 字体**（`assets/fonts/LXGWWenKaiMonoLite-Medium.ttf`）
  把技能/物品的中文名画成 PNG，产出一套可直接上传的示例图标包（见 §8）。

---

## 1. 范围

| 界面 | 是否本次做 | 图标键 |
|---|---|---|
| 对局 HUD **技能槽**（`CastKey::ALL` 那排，`main.rs` ~4433） | ✅ | `skill/<SkillId::as_u32()>` |
| 对局 HUD **物品栏**（技能栏上方那排，`main.rs` ~3626） | ✅ | `item/<ItemId::as_u32()>` |
| 学习期**商店**：物品行 / 分类列表 | ✅ | `item/<id>` |
| 学习期**技能树格子 / 技能详情** | ✅ | `skill/<id>`（多形态见 §4.3） |
| 状态图标行（buff 小图标） | ❌ 本次不做 | — |
| 记分板 / 头像 / 其它 | ❌ | — |

---

## 2. 现状

- HUD 技能槽、物品槽**目前都是纯文字**（技能名 / 物品名前 4 字 + 档位角标），代码里没有贴图。
- 客户端**没有图片加载路径**：全项目只有 `steam.rs:497` 用 `Image::from_pixels` 画 Steam 头像。
- 外部资源包机制已存在（`client/src/audio_pack.rs`，**纯 std、可单测**）：
  - 一个包 = 一个目录 + 可选清单 `circle_brawl_pack.ini`
  - 发现：本地 `%APPDATA%/warlock_brawl/audio/<id>/` + 工坊 `<Steam>/steamapps/workshop/content/<appid>/<id>/`
  - `detect_steam_root` / `workshop_roots` / `parse_library_paths` / `discover` / `publish_meta` / `is_under` 等通用件
  - 选择存 `LocalSettings`，发布/订阅走 `net-steam`
- 稳定性：`SkillId::as_u32()`（技能）与 `ItemId::as_u32()`（物品）都是**已用于网络编解码的稳定判别值**，适合当图标键。

---

## 3. 模型

- **选择值**：`icon_pack = none | <包 id>`（默认 `none`；存 `%APPDATA%/warlock_brawl/settings.txt`）。
  - `none` = 保持现状（纯文字）。
- **覆盖粒度**：**逐键覆盖**（不是「整包替换」）。包里有 `skill/36.png` 就替换火球图标，没有的就回退文字。
  这与音频「整包覆盖」不同——图标天然是「一张图对一个技能/物品」，逐键更实用。
- **热重载**：改选包 / 重扫后立即生效（图像缓存按 `(包根, 键, mtime)` 失效）。
- **不联网同步**：图标包仅本机可见；不同玩家可看到不同图标（与音频包一致，确定性无关）。
- （可选，后续低优先）一个目录同时含 `sfx/` 与 `icons/` 时，可同时出现在「音效包」与「图标包」两个列表里
  （选择仍**独立**），方便出「整套主题包」的作者；实现几乎零成本，本次先不做。

---

## 4. 包结构 + 清单

### 4.1 目录布局

```
<包根>/
  circle_brawl_pack.ini          # 可选清单：name/author/version/type/description
  icons/
    skill/<id>.png               # id = SkillId::as_u32()，如 36.png=火球、55.png=天罚
    skill/<id>_b.png             # 可选：多形态技能的 B 形态（缺则回退到 <id>.png）
    skill/63_release.png         # 可选：链激活时 S031「锁链附加·释放」（见 §4.3）
    skill/63_induce.png          # 可选：链激活时 S031「锁链附加·诱导」
    item/<id>.png                # id = ItemId::as_u32()，如 0.png=速度之靴1
  preview.png                    # 工坊预览图（可选）
  README.txt                     # 可选：本包覆盖了哪些键（见 §4.4 生成的对照表）
```

- 扩展名搜索优先级：**png → jpg → jpeg → webp**（ggez/`image` 支持范围为准，PNG 推荐）。
- **建议尺寸**：正方形，`128×128` 或 `256×256`（RGBA）；渲染时等比缩放进槽位。
- 只认**含 `icons/` 子目录**的目录为图标包（避免杂物目录被误判）。

### 4.2 清单

复用 `circle_brawl_pack.ini`（`name/author/version/description` 同音频包），`type` 新增取值：

- `type = icons | ui`（仅提示/校验；能力仍以目录为准，见音频包同款纪律）。

### 4.3 形态角标 / 变体键（**由「当前显示状态」决定**）

核心原则：**图标键与 HUD 文字用同一套状态解析**，图标跟着角标/名字走，两者不会各说各话。
技能槽当前的「显示状态」只有三种来源（`main.rs` 的 HUD 循环里已经全部算出）：

1. **A/B 形态**（配置期选定，`me.forms[s]` + `DefTable::has_alt(s)`）→ 角标文字 `目标 / 区域 / …`。
2. **链激活**（`chain_here = Some(beam)`）：此时 Y 槽**实为 S031「锁链附加」**（098c 隐藏 S019、显示 S031，
   客户端也已把 Y 键路由到 S031），按 `beam` 分两态：**释放**（`beam=false`）/ **诱导**（`beam=true`）。
3. **其余**：普通技能，取中性名。

据此定义**文件变体后缀**（ASCII；等价于「按角标选图」，但键用稳定数值+后缀，不用中文名当文件名）：

| 显示状态 | 判定 | 文件键 |
|---|---|---|
| A 形态 / 普通 | 默认 | `skill/<id>.png` |
| B 形态 | `has_alt && form_on` | `skill/<id>_b.png` |
| 链·释放 (Release) | `chain_here == Some(false)` | `skill/63_release.png` |
| 链·诱导 (Induce) | `chain_here == Some(true)` | `skill/63_induce.png` |

- 链两态用 **S031 的 id（63）**（= 链激活时客户端真正施放的技能），与「显示 S031」一致。
- **回退链**（部分包也能用，逐级降级）：
  - B 形态：`<id>_b` → `<id>`
  - 链·释放：`63_release` → `63` → `54`（S019 基础图标）
  - 链·诱导：`63_induce` → `63` → `54_b`（S019 B 图标）
  - A / 普通：`<id>`
- `_b` 沿用「第二形态」；链两态用 `_release/_induce` 显式命名，**不**把 S031 的释放/诱导硬套成 A/B。
- **实现**：把上面三种状态的解析抽成**一个纯函数**（如 `hud_slot_state(skill, form_on, chain_here) -> SlotState`），
  HUD 的**文字**与**图标**都从它取值——角标改一处、图标自动跟随，不会漂移。

> 物品同理：每个档位是独立的 `ItemId`，故**逐档给图**（`item/<id>.png`）；右上角档位数字角标保留为叠加。

### 4.4 键对照表（玩家怎么写图）

数字键对玩家不友好，因此：

- **本地包目录**：`设置 → 打开图标包目录` 时生成 `README.txt`，内含**完整 id → 中文名对照表**
  （技能表 + 物品表，由 `DefTable::def(id).name` / `ItemId::def().name` 生成）。
- 对照表需**显式列出特殊键**，否则玩家猜不到：
  - `54` = 锁链（A 钩引 / B 红链，配 `54` / `54_b`）
  - `63` = 锁链附加（配 `63_release` 释放 / `63_induce` 诱导；另可只给 `63` 兜底、或回退到 `54`/`54_b`）
  - 多形态技能（S008/S009/…/S019）都标出「B = `<id>_b`」。
- **示例包**（§8）自带同样的对照表 + 一套现成 PNG，可直接照抄改。
- 同时给一份 `keys.txt`（纯键名清单，一行一个确切文件名），方便作者批量核对/脚本化。
- （可选，后续）支持一个 `icons/index.ini` 映射别名（如 `fireball=36`），本次**不做**，先用数字 id。

---

## 5. 目录来源（复用音频包逻辑）

优先级 **本地 > 创意工坊**（同 id 本地优先，`discover` 去重）：

1. 本地：`%APPDATA%/warlock_brawl/icons/<id>/`（不依赖 Steam）。
2. 创意工坊：`<Steam>/steamapps/workshop/content/<appid>/<id>/`（文件由 Steam 客户端落地）。

Steam 库定位、多库扫描、`libraryfolders.vdf` 解析**直接复用** `audio_pack` 现有函数
（`detect_steam_root` / `workshop_roots` 等）。

> 实现取舍：把 `audio_pack` 里的**通用件**（`detect_steam_root`/`workshop_roots`/`parse_library_paths`/
> `parse_manifest`/`steam_root_from_exe`）提到一个共享模块（如 `client/src/pack_core.rs`），
> `audio_pack` 与新 `icon_pack` 各自组合自己的目录规则。避免复制粘贴或让图标耦合到音效字段。

---

## 6. 渲染接入点

新增两个客户端模块：

- `client/src/icon_pack.rs`：**纯 std** 的图标包发现/解析/键解析。可单测，不依赖 ggez。
  - `slot_state(skill, form_on, chain_here) -> SlotState`：§4.3 的显示状态解析（**与 HUD 文字同源**）。
  - `icon_path(root, state) -> Option<PathBuf>`：按 §4.3 的回退链逐级找文件，返回第一个命中的路径。
- `client/src/icons.rs`：**ggez 依赖**的图像缓存 `IconBank`：
  - `IconBank::get(ctx, path) -> Option<&Image>`（懒加载 + 缓存；失败返回 `None` 并记一次日志）
  - `IconBank::clear()`（换包/热重载时清空）
  - `draw_icon(canvas, image, rect, tint)`：等比缩放进 `rect`（保持宽高比，居中）。

接入点（均**只读**世界/客户端字段，不写 `World`）：

| 位置 | 改动 |
|---|---|
| HUD 技能槽 `main.rs` ~4433–4500 | 有图标 → 画图标（替换居中的技能名文字）；**保留**左上角按键字母、冷却倒计时、右下角形态角标。无图标 → 现状文字，一字不改。 |
| HUD 物品栏 `main.rs` ~3626–3665 | 有图标 → 画图标（替换 4 字名）；**保留**右上角档位角标。无图标 → 现状。 |
| 商店物品行（学习期） | 行首加一个小图标（约 20px），文字行照旧。 |
| 技能树格子 / 技能详情 | 技能名旁 + 详情头部加图标（可选，按格子尺寸）。 |

**冷却遮罩兼容**：现有逻辑是在槽上叠半透明矩形。图标绘制在矩形**之前**即可继续复用；
后续可选改用 `DrawParam::color` 对贴图做变暗（更美观），本次先用现有矩形，改动最小。

---

## 7. 设置 UX（主菜单「设置」）

在既有设置界面新增图标相关行（风格与音频行一致）：

| 行 | 取值 | 说明 |
|---|---|---|
| 图标包 | `无 / 各图标包` | 选中即生效 + 写回 `settings.txt`；改包热重载 |
| 要发布的图标包 | `（无）/ 各**本地**包` | 仅本地包可发布（工坊包用 `is_under` 排除） |
| 发布本地图标包 | 动作（显示上传进度） | 复用现有发布流程（创建/更新物品 → 上传） |
| 发布时复用物品 id | 开关（复用音频那套） | 映射存 `published_icon.<包id>=<fileid>` |
| 打开图标包目录 | 动作 | 创建 `%APPDATA%/warlock_brawl/icons` + 写 README（§4.4）+ 打开文件管理器 |
| 生成示例图标包 | 动作 | 调 §8 生成器，产出 `ExampleIconPack/` |

「浏览创意工坊」沿用现有行（同一个本作工坊页），不新增。

---

## 8. 示例图标包生成器（用现有字体画汉字）

目的：给工坊一个**可直接上传的实例**，同时充当「键名怎么写」的活文档。

- 输入：技能名（`DefTable::def(id).name`，中性名）+ 物品名（`ItemId::def().name`）。
- 输出：`ExampleIconPack/icons/skill/<id>.png`（含多形态 `_b`、锁链 `63_release`/`63_induce`）
  与 `.../item/<id>.png`，外加清单 + README + 预览图。
- 画法：**用 `assets/fonts/LXGWWenKaiMonoLite-Medium.ttf`**，把中文名（物品取前 2 字、技能取前 2 字，
  过长自动缩小）画在**带主题色圆角底**的正方形上，居中。
- 生成方式（**已定**）：`tools/gen_demo_icon_pack.py`（**Python + Pillow**，与现有 `tools/` 风格一致）。
  本机已确认 Pillow 12.1.1 可用；字体用仓库 `assets/fonts/LXGWWenKaiMonoLite-Medium.ttf`。
  依赖记入 `tools/README.md`（`pip install pillow`）。
- **键表来源**：客户端 `main()` 新增无窗口 CLI `--dump-icon-keys`，打印 `stem<TAB>名`（内部走 `icon_pack::key_entries()`）——
  单一事实来源，新增技能/物品时 Python 侧零维护。
- 生成器同时写 `README.txt` 的 id→名称对照表。

> 说明：示例图标只是「汉字 + 底色」的占位，不是美术成品；玩家/我们后续用真图标同名替换即可，不改代码。

---

## 9. 创意工坊 tag

- 后台新增 tag **`Icons`**（可加 `UI`）。发布图标包时打该 tag。
- tag 仅用于工坊页分类；**游戏内识别以目录布局/清单 `type` 为准**（同音频包纪律，tag 打错不影响识别）。

---

## 10. 技术要点 / 降级 / 确定性

- **解码失败 / 缺文件**：`IconBank::get` 返回 `None` → 该键回退文字，**不 panic**（CI/无声卡环境同理）。
- **性能**：懒加载 + `HashMap` 缓存；不在每帧 `load`；图标总数 ~60 张（技能+物品），内存可忽略。
- **宽高比**：等比缩放居中，避免拉伸变形。
- **确定性**：图标只读、只画；**绝不**写入 `World`/快照，**不改** `PROTOCOL_VERSION`。
- **协议/门禁**：同项目纪律——每步 `cargo build/test/clippy -D warnings`（默认与 steam 两套）。
- **版权**：工坊包由作者自负；示例包为我们程序生成，可视为 CC0，随附预览图。

---

## 11. 分步实施（每步可编译/提交/过门禁）

- [x] **I0 基础设施（纯 std + 单测）** ✔ 2026-09-27（`steam_paths.rs` / `icon_pack.rs` / `local_settings.icon_pack`）
  - 只抽**纯路径探测**到 `steam_paths.rs`（`steam_root_from_exe`/`workshop_content_root`/`parse_library_paths`/
    `workshop_roots`/`detect_steam_root`），`audio_pack` 改用它（机械移动，行为不变，靠现有音频测试兜底）。
  - 新增 `icon_pack.rs`：**自管**自己的 `IconPack`/`IconKind`/manifest（**不**复用音频的 `Pack`/`PackKind`——
    两者语义不同：逐键覆盖 vs 整包），组合 `steam_paths` 完成发现。
  - `discover` / `slot_state` / `icon_path` + 单测；`local_settings` 增 `icon_pack`（默认 `none`）+ 读写 + 单测。
- [x] **I1 HUD 技能槽图标** ✔ 2026-09-27
  - `slot_display` 纯函数（文字 / 角标 / 图标键同源，`main.rs`）+ 单测。
  - `icons.rs`（`IconBank` 懒加载缓存 + `draw_fitted` 等比绘制）；HUD 技能槽接入（有图替换中部文字、缺图回退、保留键位/冷却/形态角标）。设置 UI（选包）在 I3。
- [x] **I2 HUD 物品栏 + 商店/技能详情图标** ✔ 2026-09-27
  - HUD 物品栏：图标替换 4 字名（缺则回退），档位角标保留。
  - `ui::row_with_icon`（`row` 委托之，`None` 时完全等价）；技能树格子 + 商店物品行 + 详情头接入。
- [x] **I3 设置 UX** ✔ 2026-09-27
  - 设置页新增「图标包」（`无` / 各包，循环切换即生效 + 写回）与「打开图标包目录」两行。
  - 打开目录：创建本地根 + 写 `README.txt`（id→中文名对照与规则）+ `keys.txt`（纯文件名清单，指向 `tools/gen_demo_icon_pack.py`）。
  - **不做**游戏内「生成示例包」按钮（选 a）：生成交 Python 工具（I5）。
- [ ] **I4 工坊**：发布本地图标包 + 物品 id 复用映射 + `Icons` tag + 订阅物品列表可见（复用现有工坊覆盖层）。
- [x] **I5 示例包生成器** ✔ 2026-09-27：`tools/gen_demo_icon_pack.py`（Python + Pillow）。
  - 键表来源：客户端 `cargo run -p client -- --dump-icon-keys`（复用 `icon_pack::key_entries`，**不在 Python 重复维护**）。
  - 产物：`icons/skill/*.png` + `icons/item/*.png` + 清单 + `keys.txt` + `preview.png`；不随仓库附带生成结果。

> 建议顺序：I0 → I1 → I2 → I3 → I4 → I5。I0 的 `pack_core` 抽取是唯一有回归风险的改动，放最前、用现有测试兜底。

---

## 12. 风险 / 注意

- **抽 `steam_paths` 改到音频包**：只移动纯路径函数，必须保持音频行为完全不变（现有 `audio_pack` 单测即回归网）。
  刻意**不**统一两边的包模型（`Pack`/`PackKind`/manifest）——两者语义不同，统一收益小、回归面大。
- **图标风格不统一**：WORKSHOP 玩家素材参差；靠「等比缩放 + 统一槽底框」削弱突兀感。
- **可读性**：有图标后仍需**按键字母 + 冷却数字**（本次明确保留），避免只看图认不出技能。
- **多形态歧义**：B 形态图标缺失时会与 A 相同；建议包作者为多形态技能提供 `_b`。
- **与状态图标方案冲突**：本次明确不碰 `active_status_icons`；若将来要把 buff 也做成贴图，需先改 P4-3 的
  「不引外部贴图」约定，另行评估。

---

## 13. 记录

- 2026-09-27：初版。范围锁定「HUD 技能槽 + 物品栏 + 商店」；图标独立成包类型；内置默认无（回退文字）；
  状态图标本次不做；示例包用现有 CJK 字体程序生成；包仅存本地不同步。
