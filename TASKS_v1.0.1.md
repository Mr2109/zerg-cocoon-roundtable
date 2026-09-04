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

### A2 ⬜ novel.flow.json 双轨
- 内置 NOVEL_BLOCKS 导出为 templates/novel.flow.json（脚本/测试生成——内容逐字搬）
- 启动装载：文件优先、无文件回退编译期
- 验收：装载结果与编译期逐字段断言相等（对齐测试）

### A3 ⬜ 变量池 v2（flow_vars 表）
- 表 (sid, node_id, key, value, ts)——第 14 张表
- 节点锁定时写入产物；selector=(node_id, key) 读写 API；sys./env./conv. 前缀保留
- 验收：roundtrip 单测 + 锁定钩子联动测试

### A4 ⬜ {{node.field}} 替换 + 表单读 inputs + human_gate 声明
- 上下文组装：desc 内 {{}} 占位替换（未定义引用→装载失败 errors 表）
- 新建表单字段从模板 inputs 动态渲染（不锁死小说类型/篇幅）
- 验收：手工——新模板表单字段随模板变；引用校验测试

## B 案：节点层

### B1 ⬜ RtFlowNode trait + single/gate 节点
- trait: kind/version/execute(ctx)/retry_policy——discussion 包壳现有状态机
- single: prompt 模板+变量替换→一次 LLM 调用→写变量池
- gate: 数值/条件判定→next_by 分支选择
- 验收：mock 三节点流（discussion→single→gate）跑通

### B2 ⬜ next/next_by 受控 DAG + current_node 断点兼容
- 主循环线性→按声明推进（无 next=隐式顺序——novel 等价）
- 断点续跑记录 current_node（兼容 current_block）
- 验收：novel 改造前后 mock 全流程输出一致（等价性专测）+ 分支流测试

### B3 ⬜ tool 节点（HTTP+脚本最小集）+ gate human_confirm
- tool: HTTP 请求/本地脚本执行——参数模板+输出写变量池；dispatch 形态留位（虫族派单）
- human_confirm: 暂停=落库 awaiting_human+复位；恢复=确认后续跑；确认卡 UI
- 验收：mock HTTP 测试 + 人为造确认点手工过

### B4 ⬜ 合同审查任务流实证（Mr2109拍板）
- templates/contract.flow.json：4 角色团（司衡/司约/司账/司权）+ 6 节点（抽取→四维审查→争议辩论→人核→修改建议→报告）
- 修改建议格式对齐Mr2109惯例（红删除线/红字/蓝字民法典 585/566/497）
- 验收：Mr2109实际用一轮真合同

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
