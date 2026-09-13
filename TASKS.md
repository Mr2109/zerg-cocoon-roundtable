# 圆桌派 Rust 重写 —— 任务清单（zerg-cocoon 第一个茧）

> 基线：Web 版 2.1.0（内部项目，未随本仓发布——功能对齐参照）
> 设计：虫族主 git `docs/项目文档/v2.5.8/设计-v2.5.8-虫茧-圆桌派Rust重写-20260903.md`（十二章——含 M1-M7 自审）
> 状态约定：⬜ 待办 / 🔄 进行中 / ✅ 完成——每任务完成 = 真实验收（非自报）
> 纪律：独立 git（本仓）——分小批 add/commit（AHZ 信号 9 规避）——Web 版并行运行不中断

## 阶段 0：准备（无——Mr2109 2026-09-03 两度定：不跑 Web 版——黄金样本概念取消）

> ~~T0-1 黄金样本~~ / ~~T0-2 协作点核对~~ 均取消。
> 对齐方式（替代黄金样本）：① Web 版 **test_discussion.py 用例移植**为 Rust 引擎单测（mock——确定性——代码级对齐不跑 Web）② 移植时逐函数对照 engine.py 语义 ③ 终验 = Mr2109实际用 Rust 版跑一轮完整创作。
> Web 版代码 = 翻译源（读——不启动）——终态无 Web——全 Rust 界面（虫族 UI 虫茧）

## 阶段 1：仓库与骨架

### T1-1 ✅ crate 骨架初始化（2026-09-03）
- Cargo.toml（crate 名候选 `zerg-roundtable`——lib 形态——egui 依赖先不引——纯 lib 可编译）
- 目录：src/{lib.rs, engine/, db/, ai/, embed/, ui/}（空 mod）
- 验收：cargo build 过——lib 空接口（如 `pub fn version() -> &'static str`）
- 依赖：无

### T1-2 ✅ 依赖选型锁定（2026-09-03——Cargo.toml 分阶段启用——tokio/rusqlite/reqwest/eventsource/serde 已拉——egui/fastembed 注释待 T5/T7）
- tokio（runtime）/rusqlite（bundled）/reqwest + reqwest-eventsource/serde + serde_json/fastembed/egui + eframe（UI 阶段才引——先注释）/thiserror
- 验收：cargo build 过——依赖版本锁定 Cargo.lock 入库
- 依赖：T1-1

## 阶段 2：数据层

### T2-1 ✅ schema（10 表新库——2026-09-03——SCHEMA_SQL 对齐 Web 版 + sessions.project_type——单测 10 表+project_type 验证过）
- 对照 Web 版 database.py 表结构：sessions/messages/templates/chapters/content_chunks/discussions/token_usage/world_settings/character_state/content_embeddings——字段 Rust 化（+项目类型 project_type——引擎/模板分离）
- 验收：建库脚本跑——PRAGMA table_info 对照 Web 版表字段差异清单
- 依赖：T1-2

### T2-2 ✅ rusqlite 线程隔离层（2026-09-03——Db/Arc<Mutex<Connection>>+spawn_blocking——std Mutex 锁在线程内不跨 await——3 测试过含 10 并发写）
- DB 专用线程 + mpsc 通道（借鉴 rusqlite_isle/rhei_tokio_rusqlite 模式）——或 spawn_blocking 封装
- 验收：async 读写测试过——并发 10 请求不卡
- 依赖：T2-1

### T2-3 ✅ CRUD 核心移植（2026-09-03——models.rs 6 模型类型化——crud.rs: session/message/template(upsert+lock)/world/character/token 全方法——SQL 照抄 Web 版——6 测试过（roundtrip/模板锁定/覆盖/角色）——chapters/content_chunks 函数随 T5 章节引擎配）
- Web 版 database.py 各函数 → Rust（create_session/get_messages/upsert_template/lock_block/章节 CRUD…——逐函数对照）
- 验收：Rust 单测（对应 Web 版用例）——字段行为一致
- 依赖：T2-2

## 阶段 3：AI 层

### T3-1 ✅ trait AiProvider + mock（2026-09-03——AiMessage/AiReply——MockProvider 脚本队列——M4 测试确定性）
- `trait AiProvider { async fn chat(&self, msgs, opts) -> Result<AiReply> }`——真 provider（虫族网关）/mock provider（脚本化响应）
- 验收：mock 单测（确定性——固定响应序列）
- 依赖：T2

### T3-2 ✅ 虫族网关真 provider（2026-09-03——真调验证过：'1+1=2。' 2.53s——踩坑: 默认model错(zerg-ornith是provider名非模型名——改ornith-1.5-35b) + reqwest走死系统代理7890(Connection refused)——no_proxy()直连修复——凭据env注入不入git）
- reqwest 调虫族网关（zerg-ornith——X-Auth-Token）——流式接收（reasoning_content 处理——思考模型坑：content 空读 reasoning——继承 Web 版实测经验）
- 验收：真调 ornith 一次成功（对话往返——token 计数）
- 依赖：T3-1

## 阶段 4：引擎核心（最大阶段）

### T4-1 ✅ 项目模板数据化（2026-09-03，templates/novel.rs，13 BLOCKS+5 AUTHORS+主持人 逐字对齐 Web 版 database.py 535-578，LazyLock，单测过）
- Web 版 BLOCKS/BLOCK_DESC/AUTHORS 定义 → Rust struct 表驱动（引擎/模板分离——BLOCK 引导文案/字段/门禁规则数据）
- 验收：13 块配置数据完整——文案逐字对齐 Web 版（内容资产不动）
- 依赖：T2

### T4-2 ✅ 讨论状态机（通用引擎——全自动——草案/投票/讨论/优化/确认/锁定）
- process_block 逻辑移植（解耦：引擎只懂讨论推进——Block 内容由模板给）——3 草案 → AI 自决投票 → 主持人引导 → 讨论 → 优化 → AI 确认 → 锁定——**全自动不停**（人工只在创建时定类型/字数——M1 修正）——状态机留"人工闸"扩展点（默认放行）
- 验收：引擎单测（mock AI——走完一个 Block 全流程——状态迁移正确——自动跑完不停）
- 依赖：T3 + T4-1

### T4-3 ✅ quality_check 门禁移植
- Web 版 quality_check（评分/D 级重跑规则）→ Rust——验收：mock 用例对照 Web 版判定一致
- 依赖：T4-2

### T4-4 ✅ 断点续跑 + 进度持久化（M3）
- 每轮落库（current_block/cycle/消息）——启动扫未完成会话——恢复路径
- 验收：中途 kill——重启——续跑同位置
- 依赖：T4-2

## 阶段 5：章节/记忆

### T5-1 ✅ 章节生成移植
- generate_chapters/generate_chapter_content → Rust（对照 Web 版逻辑——含 _build_memory_context 上下文组装）
- 验收：mock 跑——产出结构对照黄金样本
- 依赖：T4

### T5-2 ✅ fastembed 向量记忆
- bge-small-zh 加载（模型文件复用/下载——90MB）——段落嵌入存 content_embeddings——余弦检索（_retrieve_relevant）
- 验收：建索引——检索命中测试（同 Web 版语义）
- 依赖：T5-1

## 阶段 6：评审/批量

### T6-1 ✅ 评审重写移植（review_chapter_content/_rewrite_chapter/_build_review_lessons）
- 验收：mock 评审流程跑通——问题清单/重写触发对照 Web 版
- 依赖：T5

### T6-2 ✅ 批量队列 + 调度器（M7）
- run_batch 移植——引擎 AI 统一调度队列（多会话排队——单槽）——暂停/终止
- 验收：2 会话批量 mock 跑——排队顺序正确——可停
- 依赖：T6-1

## 阶段 7：UI（egui）

### T7-1 ✅ UI 骨架 + Home（会话列表/新建项目类型选择）
- egui 视图框架（中文字体——复用虫族 UI 经验）——会话列表——新建（选项目类型：小说——引擎/模板分离可见）
- 验收：手工——列表显示——新建会话入库
- 依赖：T1-2 + T2

### T7-2 ✅ SessionView（三栏：Block 进度/讨论流/模板编辑锁定）
- 左 Block 进度（13 块状态）——中讨论流（消息气泡——分层渲染——作者配色——流式打字机）——右模板字段编辑/锁定——**全自动进度**（无等待人工态——M1 修正——进度条连续）
- 验收：手工跑一个 Block 全流程（mock 或真调）——自动跑完不卡
- 依赖：T7-1 + T4

### T7-3 ✅ ChapterView（章节树/正文编辑/评审）+ 批量面板
- 验收：全管线手工验收（新建→讨论→锁定→生成章节→正文→评审）
- 依赖：T7-2 + T5 + T6

## 阶段 8：虫茧集成

### T8-1 ✅ 虫族 UI 虫茧模块（2026-09-03/04——拆 a/b/c 三批——虫茧上船）
- **T8-1a（14ab5662，2026-09-03）**：zerg-roundtable path 依赖（`../../zerg-cocoon/圆桌派`——跨仓独立 git）——build_registry 加虫茧 ModuleManifest（roundtable/boxes 图标/可装卸）——app.rs roundtable 懒加载 + main_view 分支 CentralPanel 嵌 rt.render(ui)——切走引擎后台继续 M2——编译过（zerg-ui 9/3 新 bin）
- **T8-1b（a43d215d，2026-09-04）**：**平台两级语义定案（Mr2109：虫茧=平台启动器——点虫茧=平台界面应用栅格——圆桌派只是其一）**——rt_active=false 平台栅格（卡片数组静态声明——未来加茧=加卡片）/ true 圆桌派全屏+返回条——+rt_active 字段/init——zh+en yml 4 平台文案键——修 RichText.wrap 不存在（E0599）
- **T8-1c（471e050，2026-09-04）**：1 号症状修复链收尾 + ②字体按钮（A−/A+ 9-24px）+ ③模型选择器（5 候选/MODELS 常量/默认 ornith-1.5-35b/表单模型行/DB sessions.provider/make_ai(prov) 白名单校验——旧值回退防 404）+ ④细节完善（复位残留 running→idle/完成·停止落库 ✅⏸ 提示/状态中文/模型名显示）——双仓编译 EXIT=0——部署后断点续跑实证（仙侠复仇记 Block 4 锁定无损→5）
- 验收：✅ 点虫茧→平台栅格→圆桌派（返回条回栅格）——切走讨论引擎后台继续——重启断点续跑
- 依赖：T7-3（圆桌派 UI 可嵌）+ 主 git 开发

## 阶段 9：全功能对齐验证

### T9-1 ⬜ 引擎级回归（Web 版用例 → Rust 单测全过）
- test_discussion.py 用例移植——mock 全过
- 依赖：T4-T6

### T9-2 🔄 真调对齐验证（Rust 版实际跑一轮完整创作——实证中）
- Rust 版真实跑一轮（ornith——一个典型小说项目——13 Block + 正文 + 评审）——检查管线完整/产出质量（对照 Web 版 test 用例语义——不产 Web 黄金样本——Mr2109终验为准）
- 实证进展（2026-09-04）：会话 rt_6a9980e8「仙侠复仇记」真调 ornith——主持人开场+3 草案+投票+五司表态真 AI 产出——重启断点续跑（Block 4 锁定无损续 Block 5）——跑完 13 Block 待验
- 依赖：T7 + 模型在线

### T9-3 ⬜ Mr2109终验（天天用的手感——UI/流程/等待点）
- 验收：Mr2109实际用一轮完整创作——签字
- 依赖：T9-2

## 里程碑
- M1：T4-2 完成（引擎核心跑通——mock 走全 Block）
- M2：T7-3 完成（UI 全流程可手工用）
- M3：✅ T8-1 完成（2026-09-04——虫茧上船——平台两级语义——虫族 UI 里可用）
- M4：T9-3 完成（Mr2109签字——对齐通过——Web 版去留决策）

## 已定案（2026-09-03）
- 形态：A 纯 egui——彻底集成 / 新库 / fastembed / 全功能对齐 / 脑实施
- 平台：zerg-cocoon——茧=独立 git——圆桌派 = 第一个茧
- crate 名候选：zerg-roundtable（T1-1 定）

## 已定案（2026-09-04 补充）
- **平台两级语义（Mr2109 verbatim）**：虫茧=平台启动器——点虫茧=平台界面应用栅格——圆桌派只是其一——rt_active 两级（false 栅格/true 全屏+返回条）——未来加茧=加卡片
- 模型选择器生效根因=make_ai 只走 env 不读会话 provider——改表单模型行→DB sessions.provider→make_ai(prov) 白名单校验（旧值 zerg-ornith 回退 env 默认防 404）
- 细节完善自行深度思考（Mr2109 2026-09-04）：字体按钮 9-24px/复位残留 running/完成停止落库 ✅⏸/状态中文/模型名显示
