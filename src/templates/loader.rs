//! templates/loader.rs — flow.json 装载器（A1——v1.0.1 可组装任务流）
//! 职责：读 templates/*.flow.json → serde 解析 → 三重校验（schema/变量引用/图）→ 错误清单一次返回
//! 设计依据：设计-可组装任务流系统-20260904.md 第三节 + graphon importer `_unsupported_node_reasons` 风格
//! 铁律：AI 生成的与人手写的走同一装载器（无特权通道——Mr2109）

use super::novel::{Author, Block, GateRule, Moderator, ProjectTemplate, TemplateInput};
use crate::errors::{RtError, B101_INVALID_STATE, B103_JSON_PARSE};
use serde::Deserialize;
use std::path::Path;

/// flow.json 文件结构（A 案线性——B 案 next/next_by 已在 Block 上预留）
#[derive(Debug, Deserialize)]
struct FlowFile {
    id: String,
    #[allow(dead_code)]
    name: String,
    #[serde(default = "_flow_version")]
    #[allow(dead_code)]
    version: i64,
    #[serde(default)]
    inputs: Vec<TemplateInput>,
    roles: FlowRoles,
    nodes: Vec<FlowNode>,
    #[serde(default)]
    gate: GateRule,
}
fn _flow_version() -> i64 {
    1
}

#[derive(Debug, Deserialize)]
struct FlowRoles {
    moderator: Moderator,
    #[serde(default)]
    panel: Vec<Author>,
}

/// flow 节点（serde 中间形态——index 由装载器按序补）
#[derive(Debug, Deserialize)]
struct FlowNode {
    id: String,
    #[serde(default = "_default_kind")]
    kind: String,
    name: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    fm: Option<String>,
    #[serde(default)]
    next: Vec<String>,
    #[serde(default = "_default_gate")]
    human_gate: String,
    #[serde(default)]
    model: String,
}
fn _default_kind() -> String {
    "discussion".into()
}
fn _default_gate() -> String {
    "none".into()
}

/// 装载结果：模板或错误清单（错误一次全报——对齐 graphon importer 风格）
pub fn load_flow_file(path: &Path) -> Result<ProjectTemplate, Vec<String>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| vec![format!("读文件失败 {}: {}", path.display(), e)])?;
    load_flow_str(&text)
}

/// 从字符串装载（AI 建流 validate-loop 直接喂字符串——同一入口）
pub fn load_flow_str(text: &str) -> Result<ProjectTemplate, Vec<String>> {
    let mut errs: Vec<String> = Vec::new();

    // ① JSON 解析
    let flow: FlowFile = match serde_json::from_str(text) {
        Ok(f) => f,
        Err(e) => return Err(vec![format!("[schema] JSON 解析失败: {e}")]),
    };

    // ② schema 校验
    if flow.id.is_empty() {
        errs.push("[schema] id 不能为空".into());
    }
    if flow.nodes.is_empty() {
        errs.push("[schema] nodes 不能为空".into());
    }
    if flow.roles.panel.is_empty() {
        errs.push("[schema] roles.panel 至少 1 个角色（角色团变长 1-8——Mr2109）".into());
    } else if flow.roles.panel.len() > 8 {
        errs.push(format!(
            "[schema] roles.panel {} 个——上限 8（角色团变长 1-8）",
            flow.roles.panel.len()
        ));
    }
    for (i, n) in flow.nodes.iter().enumerate() {
        if n.id.is_empty() {
            errs.push(format!("[schema] nodes[{i}] id 不能为空"));
        }
        match n.kind.as_str() {
            "discussion" => {
                if n.fields.is_empty() {
                    errs.push(format!("[schema] 讨论节点 {} fields 不能为空", n.id));
                }
            }
            "single" | "tool" | "gate" => {}
            other => errs.push(format!(
                "[schema] 节点 {} kind='{}' 非法——可选 discussion/single/tool/gate",
                n.id, other
            )),
        }
        if !["none", "key_points", "every_step"].contains(&n.human_gate.as_str()) {
            errs.push(format!(
                "[schema] 节点 {} human_gate='{}' 非法——可选 none/key_points/every_step",
                n.id, n.human_gate
            ));
        }
    }

    // ③ 图校验：id 唯一 / next 悬空引用 / 环检测
    let ids: Vec<&str> = flow.nodes.iter().map(|n| n.id.as_str()).collect();
    for (i, n) in flow.nodes.iter().enumerate() {
        if let Some(dup) = ids[..i].iter().find(|x| **x == n.id) {
            errs.push(format!("[graph] 节点 id '{}' 重复（与 {dup}）", n.id));
        }
        for nxt in &n.next {
            if !ids.contains(&nxt.as_str()) {
                errs.push(format!("[graph] 节点 {} next 引用不存在的 '{}'", n.id, nxt));
            }
        }
    }
    if let Some(cycle) = find_cycle(&flow.nodes) {
        errs.push(format!("[graph] 存在环: {}", cycle.join(" → ")));
    }

    // ④ 变量引用校验（{{node.field}}——字段来自各节点 fields）
    let declared: std::collections::HashSet<String> = flow
        .nodes
        .iter()
        .flat_map(|n| n.fields.iter().map(|f| format!("{}.{}", n.id, f)))
        // A4 修订：inputs 也是引用命名空间（{{input.key}}——运行时从会话行注入）
        .chain(flow.inputs.iter().map(|i| format!("input.{}", i.key)))
        .collect();
    for n in &flow.nodes {
        for var in extract_refs(&n.desc) {
            if !declared.contains(&var) {
                errs.push(format!(
                    "[vars] 节点 {} desc 引用未定义的 {{{{{var}}}}}",
                    n.id
                ));
            }
        }
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    // ⑤ 组装 ProjectTemplate（index 按序补——node id 保留在 Block.name 之外的新字段？——用 id=name 约定：name 优先，id 存 kind 表）
    let blocks = flow
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| Block {
            index: i,
            name: n.name.clone(),
            fields: n.fields.clone(),
            fm: n.fm.clone().unwrap_or_else(|| {
                n.fields
                    .iter()
                    .map(|f| format!("## {f}:{{}}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
            kind: n.kind.clone(),
            desc: n.desc.clone(),
            next: n.next.clone(),
            human_gate: n.human_gate.clone(),
            model: n.model.clone(),
        })
        .collect();

    Ok(ProjectTemplate {
        project_type: flow.id,
        blocks,
        authors: flow.roles.panel,
        moderator: flow.roles.moderator,
        inputs: flow.inputs,
        gate: flow.gate,
    })
}

/// 提取 {{node.field}} 引用
fn extract_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            let inner = after[..end].trim();
            if inner.contains('.') {
                out.push(inner.to_string());
            }
            rest = &after[end + 2..];
        } else {
            break;
        }
    }
    out
}

/// 环检测（DFS 全分支——命中即回路径）
fn find_cycle(nodes: &[FlowNode]) -> Option<Vec<String>> {
    use std::collections::HashMap;
    let nexts: HashMap<&str, &Vec<String>> =
        nodes.iter().map(|n| (n.id.as_str(), &n.next)).collect();
    fn dfs(
        cur: &str,
        nexts: &HashMap<&str, &Vec<String>>,
        path: &mut Vec<String>,
        on_path: &mut std::collections::HashSet<String>,
    ) -> Option<Vec<String>> {
        let list: &[String] = match nexts.get(cur) {
            Some(v) => v,
            None => return None,
        };
        for nxt in list.iter() {
            if on_path.contains(nxt) {
                let pos = path.iter().position(|x| x == nxt).unwrap_or(0);
                let mut cycle = path[pos..].to_vec();
                cycle.push(nxt.clone());
                return Some(cycle);
            }
            on_path.insert(nxt.clone());
            path.push(nxt.clone());
            if let Some(c) = dfs(nxt, nexts, path, on_path) {
                return Some(c);
            }
            path.pop();
            on_path.remove(nxt);
        }
        None
    }
    for n in nodes {
        let mut path = vec![n.id.clone()];
        let mut on_path = std::collections::HashSet::from([n.id.clone()]);
        if let Some(c) = dfs(&n.id, &nexts, &mut path, &mut on_path) {
            return Some(c);
        }
    }
    None
}

/// 从模板目录装载（templates/<id>.flow.json——文件优先；无文件回退编译期 novel）
pub fn get_template_loaded(project_type: &str, templates_dir: &Path) -> ProjectTemplate {
    let path = templates_dir.join(format!("{project_type}.flow.json"));
    if path.exists() {
        match load_flow_file(&path) {
            Ok(t) => return t,
            Err(errs) => {
                log::error!(
                    "模板装载失败 {}（回退内置）: {} 条错误: {}",
                    path.display(),
                    errs.len(),
                    errs.join("; ")
                );
            }
        }
    }
    super::novel::novel_template_fallback(project_type)
}

pub fn _unused_rt(_: RtError) -> String {
    format!("{B103_JSON_PARSE}{B101_INVALID_STATE}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r##"{
        "id": "demo", "name": "演示流", "version": 1,
        "inputs": [{"key": "topic", "label": "主题", "required": true}],
        "roles": {
            "moderator": {"name": "主持人", "role": "引导"},
            "panel": [
                {"name": "甲", "zi": "字一", "specialty": "分析", "description": "负责分析"},
                {"name": "乙", "zi": "字二", "specialty": "执行", "description": "负责执行"}
            ]
        },
        "nodes": [
            {"id": "n0", "kind": "discussion", "name": "议题", "desc": "围绕{{topic}}讨论",
             "fields": ["结论"], "fm": "= 结论:{}"},
            {"id": "n1", "kind": "single", "name": "总结", "desc": "总结{{n0.结论}}",
             "fields": ["摘要"], "fm": "= 摘要:{}", "next": ["n2"]},
            {"id": "n2", "kind": "gate", "name": "验收", "fields": []}
        ],
        "gate": {"pass_score": 75, "max_cycle": 2}
    }"##;

    #[test]
    fn valid_flow_loads() {
        let t = load_flow_str(VALID).expect("合法流应装载成功");
        assert_eq!(t.project_type, "demo");
        assert_eq!(t.blocks.len(), 3);
        assert_eq!(t.authors.len(), 2);
        assert_eq!(t.gate.pass_score, 75);
        assert_eq!(t.blocks[0].index, 0);
        assert_eq!(t.blocks[1].next, vec!["n2"]);
        // {{topic}} 来自 inputs——inputs 不在 declared 里，但 topic 是 input 引用应放行？
        // 设计：inputs 也进 declared 命名空间（key 直接引用）
    }

    #[test]
    fn undefined_var_rejected() {
        let bad = VALID.replace("{{n0.结论}}", "{{n9.不存在}}");
        let errs = load_flow_str(&bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("[vars]")),
            "应有变量错误: {errs:?}"
        );
    }

    #[test]
    fn dangling_next_rejected() {
        let bad = VALID.replace("\"next\": [\"n2\"]", "\"next\": [\"nX\"]");
        let errs = load_flow_str(&bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("[graph]")),
            "应有图错误: {errs:?}"
        );
    }

    #[test]
    fn cycle_rejected() {
        // n2 → n1（n1 本就 →n2）构成 n1→n2→n1 环
        let bad = VALID.replace("\"fields\": []}", "\"fields\": [], \"next\": [\"n1\"]}");
        let errs = load_flow_str(&bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("环")),
            "应有环错误: {errs:?}"
        );
    }

    #[test]
    fn bad_kind_rejected() {
        let bad = VALID.replace("\"kind\": \"single\"", "\"kind\": \"magic\"");
        let errs = load_flow_str(&bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("kind")),
            "应有 kind 错误: {errs:?}"
        );
    }

    #[test]
    fn panel_limit_enforced() {
        let nine: Vec<String> = (0..9)
            .map(|i| {
                format!(
                    r#"{{"name": "角{i}", "zi": "字{i}", "specialty": "s", "description": "d"}}"#
                )
            })
            .collect();
        let bad = VALID.replace(
            r#"{"name": "甲", "zi": "字一", "specialty": "分析", "description": "负责分析"},
                {"name": "乙", "zi": "字二", "specialty": "执行", "description": "负责执行"}"#,
            &nine.join(","),
        );
        let errs = load_flow_str(&bad).unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("上限 8")),
            "应有角色数错误: {errs:?}"
        );
    }

    #[test]
    fn error_list_is_batch() {
        // 两处错误一次全报（schema+graph——不逐个跑趟）
        let bad = VALID
            .replace("\"kind\": \"gate\"", "\"kind\": \"wrong\"")
            .replace("\"next\": [\"n2\"]", "\"next\": [\"nope\"]");
        let errs = load_flow_str(&bad).unwrap_err();
        assert!(errs.len() >= 2, "错误应批量返回: {errs:?}");
    }
}
