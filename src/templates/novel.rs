//! templates/novel.rs — 小说项目模板（T4-1——2026-09-03）
//! 来源：Web 版 database.py BLOCKS/AUTHORS/MODERATOR（535-578——逐字对齐——内容资产不动）
//! 架构：项目模板 = 数据（引擎/模板分离——引擎不懂小说——BLOCKS 是引擎的"议题剧本"）

use serde::{Deserialize, Serialize};

/// 一个讨论 Block（= 引擎推进的议题单元——内容由模板定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub index: usize,
    pub name: String,
    pub fields: Vec<String>,
    pub fm: String, // 字段格式模板（{} 占位）
    /// 节点类型（v1.0.1 B 案——A 期 novel 全是 discussion；serde 默认向后兼容旧 JSON）
    #[serde(default = "default_kind")]
    pub kind: String,
    /// 讨论引导文案（BLOCK_DESC——v1.0.1 从 Web 版逐字搬入声明文件）
    #[serde(default)]
    pub desc: String,
    /// 跳转声明（v1.0.1 B2 DAG——空=隐式顺序）
    #[serde(default)]
    pub next: Vec<String>,
    /// human_gate 档位（none|key_points|every_step——默认 none 全自动）
    #[serde(default = "default_human_gate")]
    pub human_gate: String,
    /// 节点级模型覆盖（空=用会话 provider——模型分级）
    #[serde(default)]
    pub model: String,
}
fn default_kind() -> String {
    "discussion".into()
}
fn default_human_gate() -> String {
    "none".into()
}

impl Block {
    /// 内置模板字面量辅助——13 处 novel Block 补默认扩展字段（kind/desc/next/human_gate/model）
    fn novel(index: usize, name: &str, fields: &[&str], fm: &str) -> Self {
        Block {
            index,
            name: name.into(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
            fm: fm.into(),
            kind: default_kind(),
            desc: String::new(),
            next: Vec::new(),
            human_gate: default_human_gate(),
            model: String::new(),
        }
    }
}

/// 作者角色（讨论参与者——v1.0.1 角色团变长 1-8——Mr2109）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub zi: String,
    pub specialty: String,
    pub description: String,
}

/// 主持人（整合/引导）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Moderator {
    pub name: String,
    pub role: String,
}

/// 小说项目 13 Block（Web 版 BLOCKS 逐字对齐——LazyLock: const 不能堆分配 String）
/// A1: 用 Block::novel 辅助——扩展字段（kind/desc/next/human_gate/model）取默认——内容资产逐字不动
pub static NOVEL_BLOCKS: std::sync::LazyLock<Vec<Block>> = std::sync::LazyLock::new(|| {
    vec![
        Block::novel(0, "类型", &["类型", "篇幅"], "## 类型:{}\n## 篇幅:{}"),
        Block::novel(1, "故事核", &["故事核"], "## 故事核:{}"),
        Block::novel(
            2,
            "读者定位与基础设定",
            &[
                "核心读者画像",
                "市场锚点",
                "风格基调",
                "核心卖点",
                "类型标签",
            ],
            "## 核心读者画像:{}\n## 市场锚点:{}\n## 风格基调:{}\n## 核心卖点:{}\n## 类型标签:{}",
        ),
        Block::novel(
            3,
            "世界观",
            &[
                "时代背景",
                "地理格局",
                "社会结构",
                "硬规则限制",
                "软文化细节",
            ],
            "## 时代背景:{}\n## 地理格局:{}\n## 社会结构:{}\n## 硬规则限制:{}\n## 软文化细节:{}",
        ),
        Block::novel(4, "主角设定", &["主角列表"], "## 主角列表:\n{}"),
        Block::novel(
            5,
            "核心冲突",
            &["核心矛盾", "冲突类型", "贯穿全书的问题", "全书主线"],
            "## 核心矛盾:{}\n## 冲突类型:{}\n## 贯穿全书的问题:{}\n## 全书主线:{}",
        ),
        Block::novel(6, "故事大纲", &["大纲"], "## 大纲:\n{}"),
        Block::novel(7, "暂定书名", &["书名"], "## 书名:{}"),
        Block::novel(
            8,
            "卷结构与衔接",
            &["总字数", "卷数与分配"],
            "## 总字数:{}\n## 卷数与分配:{}",
        ),
        Block::novel(
            9,
            "骨架填肉",
            &["填充内容", "填肉大纲", "配角列表"],
            "## 填充内容:{}\n## 填肉大纲:{}\n## 配角列表:{}",
        ),
        Block::novel(
            10,
            "逐卷大纲",
            &["卷大纲", "新增角色", "叙事意图", "情感节拍", "关键场景"],
            "## 卷大纲:{}\n## 新增角色:{}\n## 叙事意图:{}\n## 情感节拍:{}\n## 关键场景:{}",
        ),
        Block::novel(11, "逐章大纲", &["章节大纲"], "## 章节大纲:{}"),
        Block::novel(12, "正文创作", &["正文内容"], "## 正文内容:{}"),
    ]
});

/// 5 作者 + 主持人（Web 版 AUTHORS/MODERATOR 逐字对齐）
pub static NOVEL_AUTHORS: std::sync::LazyLock<Vec<Author>> = std::sync::LazyLock::new(|| {
    vec![
        Author {
            name: "司世".into(),
            zi: "字观止".into(),
            specialty: "世界观派".into(),
            description: "负责设定规则/文明结构".into(),
        },
        Author {
            name: "司人".into(),
            zi: "字知微".into(),
            specialty: "人物派".into(),
            description: "负责角色心理/对话可信度".into(),
        },
        Author {
            name: "司局".into(),
            zi: "字守衡".into(),
            specialty: "结构派".into(),
            description: "负责情节框架/伏笔节奏".into(),
        },
        Author {
            name: "司言".into(),
            zi: "字琢之".into(),
            specialty: "文笔派".into(),
            description: "负责语言质感/描写".into(),
        },
        Author {
            name: "司情".into(),
            zi: "字动心".into(),
            specialty: "共情派".into(),
            description: "负责开场引力/代入感".into(),
        },
    ]
});

pub static NOVEL_MODERATOR: std::sync::LazyLock<Moderator> =
    std::sync::LazyLock::new(|| Moderator {
        name: "主持人".into(),
        role: "引导讨论、整合意见".into(),
    });

/// 项目模板注册（引擎按 project_type 取——v1.0.1 A1 去 static 化：运行时装载 flow.json）
/// 兼容期双轨：编译期内置（novel）+ 运行时声明文件（templates/*.flow.json）——文件优先
#[derive(Debug, Clone)]
pub struct ProjectTemplate {
    pub project_type: String,
    pub blocks: Vec<Block>,
    pub authors: Vec<Author>,
    pub moderator: Moderator,
    /// 会话创建时用户要填的输入声明（A4 表单动态渲染——v1.0.1）
    pub inputs: Vec<TemplateInput>,
    /// 门禁默认规则（节点可覆盖——v1.0.1）
    pub gate: GateRule,
}

/// 模板输入声明（新建会话表单字段）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TemplateInput {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
}

/// 门禁规则（quality_check 数值判定参数）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GateRule {
    #[serde(default = "default_pass_score")]
    pub pass_score: i64,
    #[serde(default = "default_max_cycle")]
    pub max_cycle: u32,
}
fn default_pass_score() -> i64 {
    80
}
fn default_max_cycle() -> u32 {
    3
}
impl Default for GateRule {
    fn default() -> Self {
        GateRule {
            pass_score: default_pass_score(),
            max_cycle: default_max_cycle(),
        }
    }
}

pub fn novel_template() -> ProjectTemplate {
    ProjectTemplate {
        project_type: "novel".into(),
        blocks: NOVEL_BLOCKS.clone(),
        authors: NOVEL_AUTHORS.clone(),
        moderator: NOVEL_MODERATOR.clone(),
        inputs: vec![TemplateInput {
            key: "length".into(),
            label: "篇幅".into(),
            required: true,
            options: vec!["短篇".into(), "中篇".into(), "长篇".into(), "超长篇".into()],
            default: Some("长篇".into()),
        }],
        gate: GateRule::default(),
    }
}

/// 取模板（未知类型回退 novel——A2 后改为先查声明文件）
pub fn get_template(project_type: &str) -> ProjectTemplate {
    match project_type {
        "novel" => novel_template(),
        _ => novel_template(),
    }
}

/// 编译期回退（loader 装载失败时兜底——未知类型一律回 novel）
pub fn novel_template_fallback(project_type: &str) -> ProjectTemplate {
    let mut t = novel_template();
    if project_type != "novel" {
        t.project_type = project_type.to_string();
    }
    t
}

/// 模板声明目录（独立跑=crate templates/；嵌入=随 DB 目录——v1.0.1 简化：两处都查）
pub fn default_templates_dir() -> std::path::PathBuf {
    // 优先 crate 相对（开发/独立跑）——其次当前目录
    let candidates = [
        std::path::PathBuf::from("templates"),
        std::path::PathBuf::from("data/templates"),
    ];
    for c in &candidates {
        if c.is_dir() {
            return c.clone();
        }
    }
    candidates[0].clone()
}

/// 按名找 Block
pub fn find_block<'a>(blocks: &'a [Block], name: &str) -> Option<&'a Block> {
    blocks.iter().find(|b| b.name == name)
}

/// A2: 内置 novel 模板导出为 flow.json 文本（写入 templates/novel.flow.json——双轨声明文件）
/// 结构对齐 loader::load_flow_str 的 FlowFile schema；B 系列引导文案（BLOCK_DESC）随 desc 字段导出
pub fn export_novel_flow_json() -> String {
    let t = novel_template();
    let nodes: Vec<serde_json::Value> = t
        .blocks
        .iter()
        .map(|b| {
            serde_json::json!({
                "id": format!("n{}", b.index),
                "kind": b.kind,
                "name": b.name,
                "desc": b.desc,
                "fields": b.fields,
                "fm": b.fm,
                "next": [],   // 隐式顺序（B2 起 DAG 显式化后由导出补全）
                "human_gate": b.human_gate,
                "model": b.model,
            })
        })
        .collect();
    let flow = serde_json::json!({
        "id": "novel",
        "name": "小说设定流水线",
        "version": 1,
        "inputs": t.inputs,
        "roles": {
            "moderator": t.moderator,
            "panel": t.authors,
        },
        "nodes": nodes,
        "gate": t.gate,
    });
    serde_json::to_string_pretty(&flow).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn novel_template_complete() {
        assert_eq!(NOVEL_BLOCKS.len(), 13, "13 Block");
        assert_eq!(NOVEL_AUTHORS.len(), 5, "5 作者");
        // 索引连续 + 名字不空 + 字段非空
        for (i, b) in NOVEL_BLOCKS.iter().enumerate() {
            assert_eq!(b.index, i, "索引连续");
            assert!(!b.name.is_empty());
            assert!(!b.fields.is_empty(), "{} 有字段", b.name);
        }
        // 文案关键点
        assert_eq!(NOVEL_AUTHORS[0].name, "司世");
        assert_eq!(NOVEL_AUTHORS[0].zi, "字观止");
        assert_eq!(NOVEL_MODERATOR.name, "主持人");
        assert_eq!(NOVEL_BLOCKS[12].name, "正文创作");
        // 模板取用
        let t = get_template("novel");
        assert_eq!(t.blocks.len(), 13);
        assert_eq!(find_block(&t.blocks, "世界观").unwrap().fields.len(), 5);
    }

    #[test]
    fn export_roundtrip_aligns_builtin() {
        // A2 双轨对齐：导出 flow.json → loader 装载 → 与编译期内置逐字段相等
        let json = export_novel_flow_json();
        assert!(!json.is_empty());
        let loaded = crate::templates::loader::load_flow_str(&json)
            .expect("内置导出的声明必须能被装载器接受（自举一致性）");
        let builtin = novel_template();
        assert_eq!(loaded.project_type, builtin.project_type);
        assert_eq!(loaded.blocks.len(), builtin.blocks.len(), "13 Block");
        assert_eq!(loaded.authors.len(), builtin.authors.len(), "5 作者");
        for (l, b) in loaded.blocks.iter().zip(builtin.blocks.iter()) {
            assert_eq!(l.name, b.name, "Block 名逐字对齐");
            assert_eq!(l.fields, b.fields, "字段逐字对齐");
            assert_eq!(l.fm, b.fm, "格式模板逐字对齐");
            assert_eq!(l.kind, b.kind);
            assert_eq!(l.human_gate, b.human_gate);
        }
        assert_eq!(loaded.moderator.name, builtin.moderator.name);
        assert_eq!(loaded.gate.pass_score, builtin.gate.pass_score);
    }

    #[test]
    fn export_writes_file_and_loader_prefers_it() {
        // 写入临时 templates/ 目录 → get_template_loaded 应装载文件版（而非回退）
        let dir = std::env::temp_dir().join(format!("rt_flow_a2_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("novel.flow.json"), export_novel_flow_json()).unwrap();
        let t = crate::templates::loader::get_template_loaded("novel", &dir);
        assert_eq!(t.project_type, "novel");
        assert_eq!(t.blocks.len(), 13);
        // 坏文件也应回退内置（错误入日志不 panic）
        std::fs::write(dir.join("novel.flow.json"), "{ broken").unwrap();
        let t2 = crate::templates::loader::get_template_loaded("novel", &dir);
        assert_eq!(t2.blocks.len(), 13, "坏声明文件回退编译期内置");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn block_and_author_definitions_parity() {
        // T9-1 ← Web::test_block_definitions + Web::test_author_definitions
        assert!(NOVEL_BLOCKS.len() >= 11, "Block 数不少于 11");
        assert_eq!(NOVEL_BLOCKS[0].name, "类型");
        assert_eq!(NOVEL_BLOCKS[0].fields.join(","), "类型,篇幅");
        let names: Vec<&str> = NOVEL_BLOCKS.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"逐章大纲"), "Block 名里应含「逐章大纲」");
        assert!(names.contains(&"正文创作"), "Block 名里应含「正文创作」");
        assert_eq!(NOVEL_AUTHORS.len(), 5, "5 位作者");
        assert_eq!(NOVEL_AUTHORS[0].name, "司世");
        assert_eq!(NOVEL_AUTHORS[4].name, "司情");
    }
}
