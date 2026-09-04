//! ui/canvas.rs — 流程画布（C4e 只读图 + C3 编辑写回）
//! 节点框（kind 配色/human_gate 标记）+ 贝塞尔连线 + 点选编辑
//! C3 编辑模型：点选节点→底部属性面板（名称/kind/human_gate/fields/desc/next 连线勾选）
//!   +「＋节点」/「🗑删除」——所有修改直接写回 draft JSON → 过装载器校验（唯一裁判）
//!   坐标不持久化（flow.json 无坐标字段——布局=拓扑分层自动算——不污染 schema）

use eframe::egui;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};

/// 画布节点（从 draft 解析）
struct CNode {
    id: String,
    name: String,
    kind: String,
    gate: bool,
    next: Vec<String>,
    /// 本节点 next 是否为隐式顺序边（引擎语义：next 空=按数组顺序）——画布浅色虚线区分
    implicit_next: bool,
    layer: usize,
    col: usize, // 同层序号（防重叠）
}

/// kind 配色+图标（discussion 紫/single 蓝/tool 橙/gate 红）
fn kind_style(kind: &str) -> (Color32, &'static str) {
    match kind {
        "discussion" => (Color32::from_rgb(150, 110, 220), "💬"),
        "single" => (Color32::from_rgb(90, 150, 230), "✏️"),
        "tool" => (Color32::from_rgb(230, 150, 70), "🔧"),
        "gate" => (Color32::from_rgb(220, 100, 100), "🚦"),
        _ => (Color32::from_rgb(140, 140, 140), "❓"),
    }
}

const KINDS: [&str; 4] = ["discussion", "single", "tool", "gate"];

/// 解析 draft JSON → 画布节点（拓扑分层）——坏 JSON 返回 Err 消息
fn parse_nodes(draft: &str) -> Result<Vec<CNode>, String> {
    let v: serde_json::Value =
        serde_json::from_str(draft).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let nodes = v
        .get("nodes")
        .and_then(|n| n.as_array())
        .ok_or("无 nodes 数组")?;
    let mut out: Vec<CNode> = Vec::new();
    for n in nodes {
        let id = n
            .get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string();
        let name = n
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("未命名")
            .to_string();
        let kind = n
            .get("kind")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string();
        let gate = n
            .get("human_gate")
            .map(|g| !g.as_str().unwrap_or("none").eq("none"))
            .unwrap_or(false);
        let next: Vec<String> = n
            .get("next")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        out.push(CNode {
            id,
            name,
            kind,
            gate,
            next,
            implicit_next: false,
            layer: 0,
            col: 0,
        });
    }
    // 引擎语义对齐：next 空 = 按数组顺序隐式推进（B2 DAG 语义）——画布补隐式边（novel 流全是这种——否则一根线都没有）
    for i in 0..out.len().saturating_sub(1) {
        if out[i].next.is_empty() {
            let target = out[i + 1].id.clone();
            out[i].next.push(target);
            out[i].implicit_next = true;
        }
    }
    // Kahn 拓扑分层（轮初快照防同轮级联——环堆末层不崩）
    let ids: Vec<String> = out.iter().map(|n| n.id.clone()).collect();
    let pointed: Vec<bool> = ids
        .iter()
        .map(|id| out.iter().any(|n| n.next.contains(id)))
        .collect();
    let mut remaining = out.len();
    let mut assigned = vec![false; out.len()];
    let mut cur_layer = 0;
    while remaining > 0 {
        let mut progressed = false;
        let snapshot = assigned.clone(); // 轮初快照——防同轮级联（前驱本轮刚分配不算）
        for i in 0..out.len() {
            if assigned[i] {
                continue;
            }
            let is_root = !pointed[i];
            let all_preds_in: bool = out
                .iter()
                .enumerate()
                .all(|(j, n)| !n.next.contains(&out[i].id) || snapshot[j]);
            if is_root || all_preds_in {
                out[i].layer = cur_layer;
                assigned[i] = true;
                remaining -= 1;
                progressed = true;
            }
        }
        if !progressed {
            // 环——剩余全堆当前层（校验器报环——画布不崩）
            for i in 0..out.len() {
                if !assigned[i] {
                    out[i].layer = cur_layer;
                    assigned[i] = true;
                    remaining -= 1;
                }
            }
        }
        cur_layer += 1;
    }
    // 同层序号
    let mut col_count = std::collections::HashMap::new();
    for n in &mut out {
        let c = col_count.entry(n.layer).or_insert(0usize);
        n.col = *c;
        *c += 1;
    }
    Ok(out)
}

/// 编辑动作（canvas_ui 内部收集——统一应用到 draft）
enum Edit {
    UpdateNode {
        id: String,
        name: String,
        kind: String,
        gate: bool,
        desc: String,
        fields: Vec<String>,
        next: Vec<String>,
    },
    Delete(String),
    AddAfter(String), // 在目标节点后插入新节点（next 接原目标的 next）
}

/// 把编辑动作应用到 draft JSON（serde_json 保留其余字段——roles/inputs/gate 不动）
fn apply_edit(draft: &str, edit: &Edit) -> Result<String, String> {
    let mut v: serde_json::Value =
        serde_json::from_str(draft).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let nodes = v
        .get_mut("nodes")
        .and_then(|n| n.as_array_mut())
        .ok_or("无 nodes 数组")?;
    match edit {
        Edit::UpdateNode {
            id,
            name,
            kind,
            gate,
            desc,
            fields,
            next,
        } => {
            let n = nodes
                .iter_mut()
                .find(|n| n.get("id").and_then(|x| x.as_str()) == Some(id.as_str()))
                .ok_or(format!("节点 {id} 不存在"))?;
            n["name"] = serde_json::Value::String(name.clone());
            n["kind"] = serde_json::Value::String(kind.clone());
            n["human_gate"] = serde_json::Value::String(if *gate {
                "confirm".to_string()
            } else {
                "none".to_string()
            });
            n["desc"] = serde_json::Value::String(desc.clone());
            n["fields"] = serde_json::Value::Array(
                fields
                    .iter()
                    .map(|f| serde_json::Value::String(f.trim().to_string()))
                    .filter(|f| !f.as_str().unwrap_or("").is_empty())
                    .collect(),
            );
            n["next"] = serde_json::Value::Array(
                next.iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            );
        }
        Edit::Delete(id) => {
            nodes.retain(|n| n.get("id").and_then(|x| x.as_str()) != Some(id.as_str()));
            // 清理所有 next 引用
            for n in nodes.iter_mut() {
                if let Some(nx) = n.get_mut("next").and_then(|x| x.as_array_mut()) {
                    nx.retain(|s| s.as_str() != Some(id.as_str()));
                }
            }
        }
        Edit::AddAfter(id) => {
            // 新 id = max(nX)+1
            let mut maxn = -1i32;
            for n in nodes.iter() {
                if let Some(nid) = n.get("id").and_then(|x| x.as_str()) {
                    if let Some(num) = nid.strip_prefix('n').and_then(|s| s.parse::<i32>().ok()) {
                        maxn = maxn.max(num);
                    }
                }
            }
            let new_id = format!("n{}", maxn + 1);
            let orig_next: Vec<String> = nodes
                .iter()
                .find(|n| n.get("id").and_then(|x| x.as_str()) == Some(id.as_str()))
                .and_then(|n| {
                    n.get("next").and_then(|x| {
                        x.as_array().map(|a| {
                            a.iter()
                                .filter_map(|s| s.as_str().map(String::from))
                                .collect()
                        })
                    })
                })
                .unwrap_or_default();
            // 新节点插到目标后：目标 next=[新id]；新节点 next=原目标的 next
            let new_node = serde_json::json!({
                "id": new_id, "name": "新节点", "kind": "single",
                "desc": "", "fields": [], "next": orig_next, "human_gate": "none", "model": ""
            });
            if let Some(t) = nodes
                .iter_mut()
                .find(|n| n.get("id").and_then(|x| x.as_str()) == Some(id.as_str()))
            {
                t["next"] =
                    serde_json::Value::Array(vec![serde_json::Value::String(new_id.clone())]);
            }
            nodes.push(new_node);
        }
    }
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// 画布视图（工作区第三个 tab——C4e 只读图 + C3 点选编辑）
pub fn canvas_ui(app: &mut crate::ui::RoundtableApp, ui: &mut egui::Ui) {
    // 选中节点（会话态——挂 Workshop）
    let mut edit_todo: Option<Edit> = None;
    let nodes = match parse_nodes(&app.workshop.draft) {
        Ok(n) => n,
        Err(e) => {
            ui.colored_label(Color32::from_rgb(230, 110, 110), format!("画布不可用：{e}"));
            return;
        }
    };
    // 工具条
    ui.horizontal(|ui| {
        ui.label(RichText::new("画布编辑:").small().weak());
        let sel = app.workshop.canvas_sel.clone();
        let has_sel = !sel.is_empty();
        if ui
            .add_enabled(has_sel, egui::Button::new("➕ 后插节点").small())
            .clicked()
        {
            edit_todo = Some(Edit::AddAfter(sel.clone()));
        }
        if ui
            .add_enabled(has_sel, egui::Button::new("🗑 删除节点").small())
            .clicked()
        {
            edit_todo = Some(Edit::Delete(sel.clone()));
        }
        if !sel.is_empty() && !nodes.iter().any(|n| n.id == sel) {
            app.workshop.canvas_sel = String::new(); // 选中的节点已被删
        }
    });
    if nodes.is_empty() {
        ui.weak("（空流程——回代码视图写 nodes 或新建空白流）");
        return;
    }
    // 布局常量
    const NODE_W: f32 = 150.0;
    const NODE_H: f32 = 54.0;
    const GAP_X: f32 = 70.0;
    const GAP_Y: f32 = 26.0;
    let max_layer = nodes.iter().map(|n| n.layer).max().unwrap_or(0);
    let max_col = nodes.iter().map(|n| n.col).max().unwrap_or(0);
    let w = (max_layer as f32 + 1.0) * (NODE_W + GAP_X) + GAP_X;
    let h = (max_col as f32 + 1.0) * (NODE_H + GAP_Y) + GAP_Y;

    egui::ScrollArea::both()
        .id_salt("ws_canvas")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (rect, _response) =
                ui.allocate_exact_size(Vec2::new(w.max(ui.available_width()), h), Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 32, 38));
            // 网格点（浅）
            let mut gx = GAP_X;
            while gx < rect.width() {
                let mut gy = GAP_Y;
                while gy < rect.height() {
                    painter.circle_filled(
                        Pos2::new(rect.left() + gx, rect.top() + gy),
                        1.0,
                        Color32::from_rgb(52, 56, 64),
                    );
                    gy += GAP_Y * 2.0;
                }
                gx += (NODE_W + GAP_X) * 2.0;
            }
            // 节点坐标
            let pos = |n: &CNode| -> Pos2 {
                Pos2::new(
                    rect.left() + GAP_X + n.layer as f32 * (NODE_W + GAP_X) + NODE_W / 2.0,
                    rect.top() + GAP_Y + n.col as f32 * (NODE_H + GAP_Y) + NODE_H / 2.0,
                )
            };
            // 连线（先画——垫底）——隐式顺序边=虚线浅色（引擎语义 next 空=顺序），显式 next=实线
            for n in &nodes {
                for nxt in &n.next {
                    if let Some(t) = nodes.iter().find(|m| &m.id == nxt) {
                        let a = pos(n);
                        let b = pos(t);
                        let start = Pos2::new(a.x + NODE_W / 2.0, a.y);
                        let end = Pos2::new(b.x - NODE_W / 2.0, b.y);
                        let mid = (start.x + end.x) / 2.0;
                        let (color, width) = if n.implicit_next {
                            (Color32::from_rgb(95, 105, 125), 1.2)
                        } else {
                            (Color32::from_rgb(120, 140, 190), 1.6)
                        };
                        let stroke = Stroke::new(width, color).into();
                        let curve = egui::epaint::CubicBezierShape {
                            points: [start, Pos2::new(mid, start.y), Pos2::new(mid, end.y), end],
                            closed: false,
                            fill: Color32::TRANSPARENT,
                            stroke,
                        };
                        painter.add(curve);
                        painter.line_segment(
                            [end, Pos2::new(end.x - 6.0, end.y - 4.0)],
                            Stroke::new(width, color),
                        );
                        painter.line_segment(
                            [end, Pos2::new(end.x - 6.0, end.y + 4.0)],
                            Stroke::new(width, color),
                        );
                    }
                }
            }
            // 节点框（可点选）
            for n in &nodes {
                let c = pos(n);
                let r = Rect::from_center_size(c, Vec2::new(NODE_W, NODE_H));
                let (color, icon) = kind_style(&n.kind);
                let selected = app.workshop.canvas_sel == n.id;
                painter.rect(
                    r,
                    6.0,
                    if selected {
                        Color32::from_rgb(58, 66, 80)
                    } else {
                        Color32::from_rgb(42, 46, 54)
                    },
                    Stroke::new(if selected { 2.5 } else { 1.5 }, color),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    Pos2::new(r.left() + 8.0, r.top() + 8.0),
                    egui::Align2::LEFT_TOP,
                    format!("{icon} {}", n.name),
                    egui::FontId::proportional(13.0),
                    Color32::from_rgb(235, 235, 235),
                );
                painter.text(
                    Pos2::new(r.left() + 8.0, r.bottom() - 8.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("{} {}", n.id, if n.gate { "⏸人工" } else { "" }),
                    egui::FontId::monospace(10.0),
                    Color32::from_rgb(150, 155, 165),
                );
                // 点击选中（用 ui.interect 在 rect 上挂点击）
                let pr = ui.interact(r, egui::Id::new(("cnode", n.id.clone())), Sense::click());
                if pr.clicked() {
                    app.workshop.canvas_sel = n.id.clone();
                }
            }
        });
    // 属性面板（选中节点——底部）
    let sel = app.workshop.canvas_sel.clone();
    if let Some(n) = nodes.iter().find(|n| n.id == sel) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("属性 {}", n.id)).strong().small());
            if ui.small_button("应用修改").clicked() {
                // 从面板缓冲读（Workshop.canvas_buf——canvas_ui 进入时已同步）
                let b = &app.workshop.canvas_buf;
                let next: Vec<String> = b
                    .next
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let fields: Vec<String> = b
                    .fields
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                edit_todo = Some(Edit::UpdateNode {
                    id: n.id.clone(),
                    name: b.name.clone(),
                    kind: b.kind.clone(),
                    gate: b.gate,
                    desc: b.desc.clone(),
                    fields,
                    next,
                });
            }
        });
        // 缓冲同步（选中变化时从 draft 重置缓冲——不覆盖正在编辑的文本）
        {
            let b = &mut app.workshop.canvas_buf;
            if b.sync_for != n.id {
                b.sync_for = n.id.clone();
                b.name = n.name.clone();
                b.kind = n.kind.clone();
                b.gate = n.gate;
                // desc/fields 从 draft 原文补（CNode 未带——直接再解析一次值）
                let v: serde_json::Value =
                    serde_json::from_str(&app.workshop.draft).unwrap_or(serde_json::Value::Null);
                let raw = v.get("nodes").and_then(|ns| ns.as_array()).and_then(|ns| {
                    ns.iter()
                        .find(|m| m.get("id").and_then(|x| x.as_str()) == Some(n.id.as_str()))
                });
                b.desc = raw
                    .and_then(|m| m.get("desc"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                b.fields = raw
                    .and_then(|m| m.get("fields"))
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                b.next = n.next.join(", ");
            }
        }
        ui.horizontal(|ui| {
            ui.small("名称");
            ui.text_edit_singleline(&mut app.workshop.canvas_buf.name);
            ui.small("kind");
            egui::ComboBox::from_id_salt("cb_kind")
                .selected_text(app.workshop.canvas_buf.kind.clone())
                .show_ui(ui, |ui| {
                    for k in KINDS {
                        ui.selectable_value(&mut app.workshop.canvas_buf.kind, k.to_string(), k);
                    }
                });
            ui.checkbox(&mut app.workshop.canvas_buf.gate, "人工门禁");
        });
        ui.horizontal(|ui| {
            ui.small("next(逗号分隔)");
            ui.text_edit_singleline(&mut app.workshop.canvas_buf.next);
            ui.small("fields(逗号分隔)");
            ui.text_edit_singleline(&mut app.workshop.canvas_buf.fields);
        });
        ui.horizontal(|ui| {
            ui.small("desc");
            ui.text_edit_singleline(&mut app.workshop.canvas_buf.desc);
        });
    } else {
        ui.weak("（点选节点编辑——属性面板）");
    }
    // 应用编辑（统一出口——写回 draft→revalidate→流水）
    if let Some(edit) = edit_todo {
        match apply_edit(&app.workshop.draft, &edit) {
            Ok(new_draft) => {
                app.workshop.draft = new_draft;
                app.workshop.revalidate();
                let what = match &edit {
                    Edit::UpdateNode { id, .. } => format!("画布改节点 {id}"),
                    Edit::Delete(id) => format!("画布删节点 {id}"),
                    Edit::AddAfter(id) => format!("画布 {id} 后加节点"),
                };
                app.collab.log_human(what);
            }
            Err(e) => app.workshop.toast = Some(format!("❌ {e}")),
        }
    }
    // 图例
    ui.horizontal(|ui| {
        ui.label(RichText::new("图例:").small().weak());
        for k in KINDS {
            let (c, icon) = kind_style(k);
            ui.label(RichText::new(format!("{icon} {k}")).small().color(c));
        }
        ui.label(RichText::new("⏸人工=human_gate").small().weak());
        ui.label(
            RichText::new("浅色线=隐式顺序(next 空=按数组顺序)")
                .small()
                .weak(),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_layers_linear() {
        let json = r##"{"nodes":[{"id":"n0","name":"A","kind":"single","next":["n1"]},{"id":"n1","name":"B","kind":"gate","human_gate":"confirm","next":[]}]}"##;
        let ns = parse_nodes(json).unwrap();
        assert_eq!(ns[0].layer, 0);
        assert_eq!(ns[1].layer, 1);
        assert!(ns[1].gate);
    }

    #[test]
    fn parse_branch_and_cycle_no_crash() {
        // 分支：n0→n1,n0→n2——n1/n2 next 空=隐式顺序（n1→n2 补边——引擎语义）
        let branch = r##"{"nodes":[{"id":"n0","name":"A","kind":"single","next":["n1","n2"]},{"id":"n1","name":"B","kind":"single","next":[]},{"id":"n2","name":"C","kind":"single","next":[]}]}"##;
        let ns = parse_nodes(branch).unwrap();
        assert_eq!(ns[1].layer, 1);
        assert_eq!(ns[2].layer, 2); // n1→n2 隐式边 → n2 依赖 n1
        assert!(ns[1].implicit_next);
        assert!(!ns[0].implicit_next);
        // 环：n0→n1→n0——不崩不挂
        let cyc = r##"{"nodes":[{"id":"n0","name":"A","kind":"single","next":["n1"]},{"id":"n1","name":"B","kind":"single","next":["n0"]}]}"##;
        let ns2 = parse_nodes(cyc).unwrap();
        assert_eq!(ns2.len(), 2);
        // 坏 JSON
        assert!(parse_nodes("{bad").is_err());
    }

    #[test]
    fn implicit_chain_linear_flow() {
        // novel 流形态：全隐式顺序（next 全空）——画布应有 n-1 条链
        let json = r##"{"nodes":[{"id":"n0","name":"A","kind":"single","next":[]},{"id":"n1","name":"B","kind":"single","next":[]},{"id":"n2","name":"C","kind":"single","next":[]}]}"##;
        let ns = parse_nodes(json).unwrap();
        assert!(ns
            .iter()
            .enumerate()
            .all(|(i, n)| { n.layer == i && (i == 2 || (n.next.len() == 1 && n.implicit_next)) }));
        assert!(!ns[2].implicit_next); // 末节点无出边
    }

    #[test]
    fn apply_edit_updates_and_preserves() {
        let base = r##"{"id":"t","nodes":[{"id":"n0","name":"A","kind":"single","desc":"d0","fields":["x"],"next":["n1"]},{"id":"n1","name":"B","kind":"gate","next":[]}],"roles":{"moderator":{"name":"M"}}}"##;
        // 更新 n0
        let upd = apply_edit(
            base,
            &Edit::UpdateNode {
                id: "n0".into(),
                name: "A2".into(),
                kind: "tool".into(),
                gate: true,
                desc: "新 desc".into(),
                fields: vec!["f1".into(), " f2 ".into()],
                next: vec![],
            },
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&upd).unwrap();
        let n0 = &v["nodes"][0];
        assert_eq!(n0["name"], "A2");
        assert_eq!(n0["kind"], "tool");
        assert_eq!(n0["human_gate"], "confirm");
        assert_eq!(n0["fields"][1], "f2"); // trim 过
        assert!(v.get("roles").is_some()); // 其余字段保留
                                           // 删除 n0——n1 还在+引用清理
        let del = apply_edit(base, &Edit::Delete("n0".into())).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&del).unwrap();
        assert_eq!(v2["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(v2["nodes"][0]["id"], "n1");
        // AddAfter：n0 后插 n2（base 里 max=n1）
        let add = apply_edit(base, &Edit::AddAfter("n0".into())).unwrap();
        let v3: serde_json::Value = serde_json::from_str(&add).unwrap();
        assert_eq!(v3["nodes"].as_array().unwrap().len(), 3);
        let n0 = v3["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "n0")
            .unwrap();
        assert_eq!(n0["next"][0], "n2");
        let n2 = v3["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "n2")
            .unwrap();
        assert_eq!(n2["next"][0], "n1"); // 接走原目标 next
    }
}
