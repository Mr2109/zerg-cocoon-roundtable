//! ui/collab.rs — 协作者面板（C4c——人机共用工作台的 AI 座位）
//! 模式（Dify graph-diff + n8n useCodeDiff 范式）：
//!   AI 提议 = 节点级 diff 预览卡片（added/removed/changed 三色）→ 人「采纳/拒绝」
//!   采纳后走装载器校验——与手改同一条管道——无特权通道
//!   变更流水：人改/AI 改全部入账（谁/何时/改了什么）

use eframe::egui;
use egui::{Color32, RichText};

/// diff 类型
#[derive(Debug, Clone, PartialEq)]
pub enum DiffKind {
    Added,
    Removed,
    Changed,
}

/// 节点级 diff（Dify graph-diff 思想——按 node id 浅比较 data）
#[derive(Debug, Clone)]
pub struct NodeDiff {
    pub kind: DiffKind,
    pub node_id: String,
    pub node_name: String,
}

/// 一条 AI 提议（diff 卡片）
#[derive(Debug, Clone)]
pub struct Proposal {
    pub id: u64,
    pub summary: String,  // AI 一句话说明
    pub new_json: String, // AI 修改后的完整 flow.json
    pub diffs: Vec<NodeDiff>,
    pub errs: Vec<String>, // 装载器校验结果（采纳前已预检）
    pub status: ProposalStatus,
    pub ts: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProposalStatus {
    Pending,  // 待人裁决
    Applied,  // 已采纳
    Rejected, // 已拒绝
}

/// 变更流水一条
#[derive(Debug, Clone)]
pub struct ChangeLog {
    pub who: &'static str, // 人/AI
    pub what: String,
    pub ts: String,
}

/// 协作者状态
pub struct Collab {
    /// 提议卡片列表（最新在前）
    pub proposals: Vec<Proposal>,
    /// 变更流水（最新在前——cap 100）
    pub changes: Vec<ChangeLog>,
    next_id: u64,
}

impl Default for Collab {
    fn default() -> Self {
        Self::new()
    }
}

impl Collab {
    pub fn new() -> Self {
        Collab {
            proposals: Vec::new(),
            changes: Vec::new(),
            next_id: 1,
        }
    }

    /// 人改了（手编 JSON 保存成功时记一笔）
    pub fn log_human(&mut self, what: String) {
        self.changes.insert(
            0,
            ChangeLog {
                who: "人",
                what,
                ts: now_hm(),
            },
        );
        self.changes.truncate(100);
    }

    /// AI 出提议（调用方：new_json + summary——diff 在此计算+装载器预检）
    pub fn propose(&mut self, summary: String, new_json: String, base_json: &str) {
        let diffs = diff_graphs(base_json, &new_json);
        let errs = match loader_check(&new_json) {
            Ok(()) => Vec::new(),
            Err(mut e) => std::mem::take(&mut e),
        };
        self.proposals.insert(
            0,
            Proposal {
                id: self.next_id,
                summary,
                new_json,
                diffs,
                errs,
                status: ProposalStatus::Pending,
                ts: now_hm(),
            },
        );
        self.next_id += 1;
    }

    /// 采纳提议（返回新 JSON 给工作台替换草稿——调用方再走装载器+保存）
    pub fn apply(&mut self, id: u64) -> Option<String> {
        let p = self.proposals.iter_mut().find(|p| p.id == id)?;
        if p.status != ProposalStatus::Pending || !p.errs.is_empty() {
            return None;
        }
        p.status = ProposalStatus::Applied;
        let json = p.new_json.clone();
        self.changes.insert(
            0,
            ChangeLog {
                who: "AI",
                what: format!("提议 #{} 已采纳（{}）", id, p.summary),
                ts: now_hm(),
            },
        );
        self.changes.truncate(100);
        Some(json)
    }

    /// 拒绝提议
    pub fn reject(&mut self, id: u64) {
        if let Some(p) = self.proposals.iter_mut().find(|p| p.id == id) {
            if p.status == ProposalStatus::Pending {
                p.status = ProposalStatus::Rejected;
                self.changes.insert(
                    0,
                    ChangeLog {
                        who: "人",
                        what: format!("拒绝提议 #{}", id),
                        ts: now_hm(),
                    },
                );
            }
        }
    }
}

/// 节点级 diff（Dify graph-diff 同语义：按 node id——JSON data 相等比较；edges 忽略防重排误报）
fn diff_graphs(base_json: &str, next_json: &str) -> Vec<NodeDiff> {
    let (Ok(b), Ok(n)) = (
        serde_json::from_str::<serde_json::Value>(base_json),
        serde_json::from_str::<serde_json::Value>(next_json),
    ) else {
        return Vec::new();
    };
    let node_map = |v: &serde_json::Value| -> std::collections::HashMap<String, String> {
        v.get("nodes")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|nd| {
                        let id = nd.get("id")?.as_str()?.to_string();
                        // 用 name+kind+desc+fields 组合做变更指纹（等价 data JSON 比较）
                        let fp = format!(
                            "{}|{}|{}|{:?}",
                            nd.get("name").and_then(|x| x.as_str()).unwrap_or(""),
                            nd.get("kind").and_then(|x| x.as_str()).unwrap_or(""),
                            nd.get("desc").and_then(|x| x.as_str()).unwrap_or(""),
                            nd.get("fields")
                        );
                        Some((id, fp))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let base = node_map(&b);
    let next = node_map(&n);
    let mut out = Vec::new();
    for (id, fp) in &next {
        let name = next_name(&n, id);
        match base.get(id) {
            None => out.push(NodeDiff {
                kind: DiffKind::Added,
                node_id: id.clone(),
                node_name: name,
            }),
            Some(old) if old != fp => out.push(NodeDiff {
                kind: DiffKind::Changed,
                node_id: id.clone(),
                node_name: name,
            }),
            _ => {}
        }
    }
    for (id, _) in &base {
        if !next.contains_key(id) {
            let name = next_name(&b, id);
            out.push(NodeDiff {
                kind: DiffKind::Removed,
                node_id: id.clone(),
                node_name: name,
            });
        }
    }
    out
}

fn next_name(v: &serde_json::Value, id: &str) -> String {
    v.get("nodes")
        .and_then(|x| x.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|nd| nd.get("id").and_then(|x| x.as_str()) == Some(id))
                .and_then(|nd| nd.get("name").and_then(|x| x.as_str()))
        })
        .unwrap_or("?")
        .to_string()
}

/// 装载器预检（OK=() / Err=错误清单）
fn loader_check(json: &str) -> Result<(), Vec<String>> {
    crate::templates::loader::load_flow_str(json).map(|_| ())
}

fn now_hm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// 协作者面板渲染（工坊右侧 SidePanel 调用）
pub fn collab_panel(app: &mut crate::ui::RoundtableApp, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.label(RichText::new("协作者").strong());
    ui.separator();

    // AI 提议卡片区（最新在上）
    ui.label(RichText::new("AI 提议").small().strong());
    if app.collab.proposals.is_empty() {
        ui.label(
            RichText::new("（无——C4d 接入后 AI 修改会在此出卡片）")
                .weak()
                .small(),
        );
    }
    let mut apply_id: Option<u64> = None;
    let mut reject_id: Option<u64> = None;
    for p in &app.collab.proposals {
        let (status_text, status_color) = match p.status {
            ProposalStatus::Pending => ("待裁决", Color32::from_rgb(220, 180, 90)),
            ProposalStatus::Applied => ("已采纳", Color32::from_rgb(110, 200, 120)),
            ProposalStatus::Rejected => ("已拒绝", Color32::GRAY),
        };
        egui::Frame::default()
            .fill(Color32::from_rgb(40, 40, 46))
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("提议 #{}", p.id)).strong().small());
                    ui.label(RichText::new(p.ts.as_str()).weak().small());
                    ui.label(RichText::new(status_text).small().color(status_color));
                });
                ui.label(RichText::new(&p.summary).small());
                // diff 三色标注
                if p.diffs.is_empty() {
                    ui.label(RichText::new("  （无节点级差异）").weak().small());
                }
                for d in &p.diffs {
                    let (tag, color) = match d.kind {
                        DiffKind::Added => ("＋新增", Color32::from_rgb(110, 200, 120)),
                        DiffKind::Removed => ("－删除", Color32::from_rgb(230, 110, 110)),
                        DiffKind::Changed => ("～修改", Color32::from_rgb(220, 180, 90)),
                    };
                    ui.label(
                        RichText::new(format!("  {tag} {}（{}）", d.node_name, d.node_id))
                            .small()
                            .color(color),
                    );
                }
                // 校验预检结果
                if p.errs.is_empty() {
                    ui.label(
                        RichText::new("  ✅ 装载器预检通过")
                            .small()
                            .color(Color32::from_rgb(110, 200, 120)),
                    );
                } else {
                    for e in p.errs.iter().take(3) {
                        ui.label(
                            RichText::new(format!("  ❌ {e}"))
                                .small()
                                .color(Color32::from_rgb(230, 140, 140)),
                        );
                    }
                }
                if p.status == ProposalStatus::Pending {
                    ui.horizontal(|ui| {
                        let can_apply = p.errs.is_empty();
                        if ui
                            .add_enabled(can_apply, egui::Button::new("✅ 采纳").small())
                            .clicked()
                        {
                            apply_id = Some(p.id);
                        }
                        if ui.small_button("❌ 拒绝").clicked() {
                            reject_id = Some(p.id);
                        }
                    });
                }
            });
        ui.add_space(4.0);
    }
    if let Some(id) = apply_id {
        if let Some(new_json) = app.collab.apply(id) {
            app.workshop.draft = new_json;
            app.workshop.revalidate();
            app.workshop.toast = Some("✅ 已采纳 AI 提议——记得保存".into());
        }
    }
    if let Some(id) = reject_id {
        app.collab.reject(id);
    }

    ui.separator();
    // 变更流水（共同账——人改/AI 改全入账）
    ui.label(RichText::new("变更流水").small().strong());
    egui::ScrollArea::vertical()
        .id_salt("collab_changes")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.collab.changes.is_empty() {
                ui.label(RichText::new("（无变更）").weak().small());
            }
            for c in app.collab.changes.iter().take(30) {
                let who_color = match c.who {
                    "AI" => Color32::from_rgb(140, 180, 240),
                    _ => Color32::from_rgb(200, 200, 200),
                };
                ui.label(
                    RichText::new(format!("[{}] {} {}", c.ts, c.who, c.what))
                        .small()
                        .color(who_color),
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r##"{"nodes":[{"id":"n0","name":"议题","kind":"discussion","desc":"d","fields":["结论"]}]}"##;
    const CHANGED: &str = r##"{"nodes":[{"id":"n0","name":"议题","kind":"discussion","desc":"改了","fields":["结论"]},{"id":"n1","name":"新增","kind":"single","desc":"","fields":["摘要"]}]}"##;

    #[test]
    fn diff_detects_added_and_changed() {
        let diffs = diff_graphs(BASE, CHANGED);
        assert!(diffs
            .iter()
            .any(|d| d.kind == DiffKind::Added && d.node_id == "n1"));
        assert!(diffs
            .iter()
            .any(|d| d.kind == DiffKind::Changed && d.node_id == "n0"));
    }

    #[test]
    fn diff_detects_removed() {
        let diffs = diff_graphs(CHANGED, BASE);
        assert!(diffs
            .iter()
            .any(|d| d.kind == DiffKind::Removed && d.node_id == "n1"));
    }

    #[test]
    fn propose_prevalidates() {
        let mut c = Collab::new();
        // 残缺 flow（无 id/roles）——装载器预检应报 schema 错——不可采纳
        c.propose("加一个节点".into(), CHANGED.to_string(), BASE);
        assert_eq!(c.proposals.len(), 1);
        assert!(!c.proposals[0].errs.is_empty(), "残缺 JSON 预检应报错");
        assert!(c.apply(c.proposals[0].id).is_none(), "预检不过不可采纳");
    }

    #[test]
    fn proposal_rejected_then_ignored() {
        let mut c = Collab::new();
        c.propose("x".into(), CHANGED.to_string(), BASE);
        let id = c.proposals[0].id;
        c.reject(id);
        assert_eq!(c.proposals[0].status, ProposalStatus::Rejected);
        assert!(c.apply(id).is_none(), "已拒绝不可采纳");
    }
}
