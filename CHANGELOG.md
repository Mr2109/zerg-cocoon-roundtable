# 变更日志 —— 圆桌派（`zerg-roundtable`）

> 体例：一个版本一节，**倒序**；只记「可感知的变化 + 对应实证」，不复述过程。
> 未闭事项在 `TASKS.md`，本文件只记「已发生」。

## v1.0.1 —— 可组装任务流系统（A/B/C 一次成型）

**实施收官 2026-09-05；收版发布 2026-09-13。**

### A 模板层
- `ProjectTemplate` 去 static 化；模板声明**文件化**（`templates/*.flow.json`），装载器三重校验：
  schema / `{{node.field}}` 变量引用 / 图环 DFS，错误清单一次性批量返回。
- 变量池表 `flow_vars`（selector = `(node_id, key)`）；会话表单由模板 `inputs` 驱动渲染。

### B 节点层
- `RtFlowNode` trait（`kind` / `version` / `retry_policy` 自描述）+ single / gate / tool 三类节点。
- 受控 DAG：`next` / `next_by` 游标推进（小说模板全隐式线性，与旧行为逐字等价）。
- `human_gate` 人在环：挂起 → 界面确认卡 → 按裁决续跑。
- tool 节点最小集：`http <URL>` / `sh <脚本>`（30s 超时、64KB 截断）。

### C 画布层
- 画布视图（拓扑分层 + 节点类型配色 + 贝塞尔连线）→ 画布编辑写回（点选属性面板、增删节点）。
- AI 建流回路：生成 → 装载器校验 → 错误回喂修复；重复指纹熔断（上限对齐 n8n）。

### 日志与报错
- JSONL + `errors` 表双通道落盘（保留 15 天）；`RtError` 四分类；AI 调用耗时与重试次数可观测。

### 验证
- `cargo test`：收官时 71 passed；收版实跑 **71 passed / 0 failed / 1 ignored**。
- 公开仓 CI（macOS `cargo test`）在 tag 所在提交上 success。

### 公开与发布
- 独立公开仓 `Mr2109/zerg-cocoon-roundtable`；tag `v1.0.1` + Release（**0 资产**：本茧无独立可执行制品）。
- 发布前完成工作树与历史的两轮清洗（逐笔树 / 提交信息 / 提交身份三面）。

## 未发行（开发中）

- **2026-09-14 · T9-1 引擎级回归收官**：Web 版 `test_discussion.py` **14 用例逐条对齐**，
  新增 7 个用例 ⇒ `cargo test` **78 passed / 0 failed / 1 ignored**；两处 API 差异如实记录
  （`create_chapter` 返回值、章节 title/outline 无编辑入口）—— 映射表与对齐缺口见 `TASKS.md`。
