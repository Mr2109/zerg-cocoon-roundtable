# 圆桌派 v1.0.1 任务清单——可组装任务流系统（A/B/C 一次成型）

> 设计：docs/设计-可组装任务流系统-20260904.md（定稿——九章——四确认全落）
> 范围：A 模板声明文件化+变量池 → B 四类节点+受控 DAG+human_gate → C 画布+AI 建流回路；B4=合同审查实证
> 状态约定：⬜ 待办 / 🔄 进行中 / ✅ 完成——每任务完成=真实验收（测试/手工），禁自报
> 铁律：novel 模板全程回归（内容资产逐字不动）；每任务独立 commit（write-tree 链）；cargo test 全过才算完成

## A 案：模板层

### A1 ✅ ProjectTemplate 去 static 化 + 装载器骨架（2026-09-04）
- ProjectTemplate 字段 Vec/String 拥有化 + inputs/gate 新字段（TemplateInput/GateRule）
- Block 扩展字段（kind/desc/next/human_gate/model——serde default 向后兼容）+ Block::novel 辅助（13 处内置字面量内容逐字不动）
- loader.rs：load_flow_str/file——三重校验（schema/变量引用{{node.field}}/图环 DFS）——错误清单一次批量返回
- 角色团变长校验（1-8——Mr2109）
- 验收：✅ 50 测试全过（新增 loader 7 测：合法装载/未定义变量/悬空 next/环/非法 kind/角色上限/错误批量）

### A2 ✅ novel.flow.json 双轨（2026-09-04）
- export_novel_flow_json（examples/export_novel_flow.rs 一次性导出）→ templates/novel.flow.json（13 节点/5 角色 5462B 入库）
- get_template_loaded：文件优先+坏文件回退编译期（错误入日志不 panic）
- 引擎/调度器/UI 三处调用点全切「按会话 project_type 装载」——不再固定 novel
- 验收：✅ 52 测试全过（新增 roundtrip 对齐=装载==编译期逐字段 + 文件优先/坏文件回退 2 测）

### A3 ✅ 变量池 v2（flow_vars 表）（2026-09-04）
- 第 14 张表 flow_vars (sid, node_id, key, value, ts)——UNIQUE(sid,node_id,key) upsert 覆写
- crud: set_flow_var/get_flow_var/get_flow_vars（selector=(node_id,key)——graphon VariablePool 语义）
- 引擎锁定钩子：lock_up 每字段锁定时写变量池（node_id=n{index}）——B 案 {{}} 替换数据源
- 验收：✅ 53 测试全过（roundtrip：覆写/顺序/会话隔离/未定义 None）

### A4 ✅ {{node.field}} 替换 + 表单读 inputs + human_gate 声明（2026-09-04）
- substitute_vars/extract_refs（discussion.rs——未命中保持原样不静默丢信息）
- process_block 开场组装变量表（flow_vars + input.topic/length/novel_type）→ desc 替换后作为主持人引导消息注入讨论流
- 新建表单动态渲染：form_inputs/form_values——模板 inputs 驱动（options=选择行/空=自由文本）——LENGTHS 常量删除
- 验收：✅ 55 测试全过（+2 替换/提取）——手工验收待新模板（B4 合同审查流首个实证）

## B 案：节点层

### B1 ✅ RtFlowNode trait + single/gate 节点（2026-09-04）
- nodes.rs: RtFlowNode trait（kind/version/retry_policy 自描述——graphon Node 思想——Box::pin async）
- SingleNode: prompt 变量替换→一次 LLM 调用→产出写变量池（单字段全文/多字段按行解析）
- GateNode: desc 结构化判定（`值 包含 kw` / `值 非空` / 无条件）→判定结果写变量池——不重试策略
- discussion 节点暂不过 trait（等价性保护——B2 主循环改造统一分发）
- loader 修订: inputs 纳入 {{input.key}} 引用命名空间（A4 设计补全）
- 验收：✅ 59 测试全过——flow_test 三节点流端到端（single→single→gate 通过+不通过两向）

### B2 ✅ next/next_by 受控 DAG + current_node 断点兼容（2026-09-04）
- run_discussion 主循环 for→while 游标推进：next 声明优先跳转（next[0]→索引解析），空=顺序 +1（novel 全隐式线性——完全等价）
- 按 kind 分发：discussion 走 process_block 原状态机；single/gate 走 RtFlowNode（统一 NodeOutcome）
- 质量门禁限 discussion 节点（single/gate 无草案不可评分）；异常/停止进度留当前块断点续跑语义保持
- 环保底双保险（loader 拒环 + 推进步数超限终止）
- 验收：✅ 60 测试全过——novel 等价性三专测（完整跑/断点续跑/门禁重跑）原样通过 + next 跳转专测

### B3 ✅ tool 节点（HTTP+脚本最小集）+ gate human_confirm（2026-09-04）
- ToolNode: desc 语法 `http <URL>` / `sh <脚本>`——变量替换后执行——30s 超时/64KB 截断/spawn_blocking——产出写变量池；dispatch 形态（虫族派单）留位
- gate human_confirm: human_gate≠none 且无裁决→挂起（状态 awaiting_human+确认请求落讨论流）——UI 确认卡（✅批准/❌驳回）写 flow_vars 人工裁决+状态回 idle——引擎续跑按裁决走
- 优雅退出：挂起时 RunSummary completed=false（不完成不失败——awaiting_human 天然接入断点续跑）
- status_cn 加「待确认」
- 验收：✅ 61 测试全过——human_confirm 挂起→裁决→续跑端到端 + tool 节点编译验收（sh/http 实际执行留 B4 合同流实测）

### B4 🔄 合同审查任务流实证（Mr2109拍板）
- ✅ templates/contract.flow.json（4 角色团：司衡/司约/司账/司权 + 6 节点：抽取→四维审查→争议辩论→人核→修改建议→报告）——loader 验证装载成功（6 节点/4 角色）
- ✅ inputs 值入变量池（create() form_values → flow_vars input.* 前缀）——run 时统一拉取注入 {{input.xxx}}
- ✅ 修改建议格式对齐Mr2109惯例（红删除线/红字/蓝字民法典引用）——写入 desc 引导文案
- ✅ 模板选择器（新建表单 ComboBox 扫描 templates/*.flow.json——切换刷新 inputs/类型行随模板显隐——create 用选中 project_type）
- 🔄 待Mr2109实际用一轮真合同终验
- 验收：61 测试全过 + 装载器实证通过

## C 案：编排层

### C1 ⬜ 只读画布
- egui 画布渲染 flow.json（节点框/连线/human_gate 标记）+ Mermaid 导出
- 验收：手工核对图形与声明一致

### C2 ⬜ AI 建流回路（本期重心）
- 规划=圆桌讨论（需求→角色团辩论出结构化方案）+ skill 检索注入（kb 体系）
- 确认卡（Mermaid 图+人话步骤）→ 生成 flow.json → validate-loop（≤10 次+重复指纹熔断 3 次）
- 验收：一句需求让 AI 建一条流并跑通

### C3 ⬜ 画布编辑 + 模板库
- 拖拽/连线/属性面板写回 flow.json；模板库列表/复制/删除/导入导出
- 验收：手工组一条新流跑通

## 里程碑
- MA：A1-A4（模板层通——手写 JSON 可组新流）
- MB：B1-B4（节点层通——合同审查实证跑通）
- MC：C1-C3（编排层通——AI 建流闭环）

## 定案备忘（2026-09-04）
- 角色团变长 1-8（Mr2109）；validate-loop 上限 10 对齐 n8n+重复指纹 3 次熔断；human_gate 默认 none；B4=合同审查
- v1.0.1 期间收尾遗留：T9-1 引擎级回归（Web 用例移植）、T9-2 仙侠复仇记跑完、T9-3 Mr2109终验——与任务流并行不冲突
