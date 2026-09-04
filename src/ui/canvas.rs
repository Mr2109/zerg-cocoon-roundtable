//! ui/canvas.rs — 流程画布（C4e——只读图形视图）
//! Dify reactflow 范式的 egui 实现：节点框（kind 配色/human_gate 标记）+ 连线
//! 布局=拓扑分层（最长路径定列）——从 draft JSON 实时解析（跟随代码视图同步）
//! 只读起步：画布看结构、代码改内容——改回 C3（拖拽编辑）再升级

use eframe::egui;
use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};

/// 画布节点（从 draft 解析）
struct CNode {
    id: String,
    name: String,
    kind: String,
    gate: bool,
    next: Vec<String>,
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
            layer: 0,
            col: 0,
        });
    }
    // 拓扑分层：入度 0（无被指）为根——BFS 最长路径
    let ids: Vec<String> = out.iter().map(|n| n.id.clone()).collect();
    let pointed: Vec<bool> = ids
        .iter()
        .map(|id| out.iter().any(|n| n.next.contains(id)))
        .collect();
    // Kahn 分层
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
            // 前驱全部已分配（或自己是根）→ 本层
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
            // 环——剩余全堆最后一层（校验器会报环——画布不崩）
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

/// 画布视图（工作区第三个 tab）
pub fn canvas_ui(app: &mut crate::ui::RoundtableApp, ui: &mut egui::Ui) {
    let nodes = match parse_nodes(&app.workshop.draft) {
        Ok(n) => n,
        Err(e) => {
            ui.colored_label(Color32::from_rgb(230, 110, 110), format!("画布不可用：{e}"));
            return;
        }
    };
    if nodes.is_empty() {
        ui.weak("（空流程）");
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
            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(w.max(ui.available_width()), h),
                egui::Sense::hover(),
            );
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
            // 连线（先画——垫底）
            for n in &nodes {
                for nxt in &n.next {
                    if let Some(t) = nodes.iter().find(|m| &m.id == nxt) {
                        let a = pos(n);
                        let b = pos(t);
                        let start = Pos2::new(a.x + NODE_W / 2.0, a.y);
                        let end = Pos2::new(b.x - NODE_W / 2.0, b.y);
                        // 贝塞尔（水平出——水平进——同层/跨层都平滑）
                        let mid = (start.x + end.x) / 2.0;
                        let curve = egui::epaint::CubicBezierShape {
                            points: [start, Pos2::new(mid, start.y), Pos2::new(mid, end.y), end],
                            closed: false,
                            fill: Color32::TRANSPARENT,
                            stroke: Stroke::new(1.6, Color32::from_rgb(120, 140, 190)).into(),
                        };
                        painter.add(curve);
                        // 箭头
                        painter.line_segment(
                            [end, Pos2::new(end.x - 6.0, end.y - 4.0)],
                            Stroke::new(1.6, Color32::from_rgb(120, 140, 190)),
                        );
                        painter.line_segment(
                            [end, Pos2::new(end.x - 6.0, end.y + 4.0)],
                            Stroke::new(1.6, Color32::from_rgb(120, 140, 190)),
                        );
                    }
                }
            }
            // 节点框
            for n in &nodes {
                let c = pos(n);
                let r = Rect::from_center_size(c, Vec2::new(NODE_W, NODE_H));
                let (color, icon) = kind_style(&n.kind);
                painter.rect(
                    r,
                    6.0,
                    Color32::from_rgb(42, 46, 54),
                    Stroke::new(1.5, color),
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
            }
            let _ = response;
        });
    // 图例
    ui.horizontal(|ui| {
        ui.label(RichText::new("图例:").small().weak());
        for k in ["discussion", "single", "tool", "gate"] {
            let (c, icon) = kind_style(k);
            ui.label(RichText::new(format!("{icon} {k}")).small().color(c));
        }
        ui.label(RichText::new("⏸人工=human_gate").small().weak());
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
        // 分支：n0→n1,n0→n2
        let branch = r##"{"nodes":[{"id":"n0","name":"A","kind":"single","next":["n1","n2"]},{"id":"n1","name":"B","kind":"single","next":[]},{"id":"n2","name":"C","kind":"single","next":[]}]}"##;
        let ns = parse_nodes(branch).unwrap();
        assert_eq!(ns[1].layer, 1);
        assert_eq!(ns[2].layer, 1);
        assert_eq!(ns[1].col, 0);
        assert_eq!(ns[2].col, 1);
        // 环：n0→n1→n0——不崩不挂
        let cyc = r##"{"nodes":[{"id":"n0","name":"A","kind":"single","next":["n1"]},{"id":"n1","name":"B","kind":"single","next":["n0"]}]}"##;
        let ns2 = parse_nodes(cyc).unwrap();
        assert_eq!(ns2.len(), 2);
        // 坏 JSON
        assert!(parse_nodes("{bad").is_err());
    }
}
