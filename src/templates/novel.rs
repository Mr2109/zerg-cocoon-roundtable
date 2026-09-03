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
}

/// 作者角色（讨论参与者）
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
pub static NOVEL_BLOCKS: std::sync::LazyLock<Vec<Block>> = std::sync::LazyLock::new(|| vec![
    Block { index: 0, name: "类型".into(), fields: vec!["类型".into(), "篇幅".into()], fm: "## 类型:{}\n## 篇幅:{}".into() },
    Block { index: 1, name: "故事核".into(), fields: vec!["故事核".into()], fm: "## 故事核:{}".into() },
    Block { index: 2, name: "读者定位与基础设定".into(), fields: vec!["核心读者画像".into(), "市场锚点".into(), "风格基调".into(), "核心卖点".into(), "类型标签".into()], fm: "## 核心读者画像:{}\n## 市场锚点:{}\n## 风格基调:{}\n## 核心卖点:{}\n## 类型标签:{}".into() },
    Block { index: 3, name: "世界观".into(), fields: vec!["时代背景".into(), "地理格局".into(), "社会结构".into(), "硬规则限制".into(), "软文化细节".into()], fm: "## 时代背景:{}\n## 地理格局:{}\n## 社会结构:{}\n## 硬规则限制:{}\n## 软文化细节:{}".into() },
    Block { index: 4, name: "主角设定".into(), fields: vec!["主角列表".into()], fm: "## 主角列表:\n{}".into() },
    Block { index: 5, name: "核心冲突".into(), fields: vec!["核心矛盾".into(), "冲突类型".into(), "贯穿全书的问题".into(), "全书主线".into()], fm: "## 核心矛盾:{}\n## 冲突类型:{}\n## 贯穿全书的问题:{}\n## 全书主线:{}".into() },
    Block { index: 6, name: "故事大纲".into(), fields: vec!["大纲".into()], fm: "## 大纲:\n{}".into() },
    Block { index: 7, name: "暂定书名".into(), fields: vec!["书名".into()], fm: "## 书名:{}".into() },
    Block { index: 8, name: "卷结构与衔接".into(), fields: vec!["总字数".into(), "卷数与分配".into()], fm: "## 总字数:{}\n## 卷数与分配:{}".into() },
    Block { index: 9, name: "骨架填肉".into(), fields: vec!["填充内容".into(), "填肉大纲".into(), "配角列表".into()], fm: "## 填充内容:{}\n## 填肉大纲:{}\n## 配角列表:{}".into() },
    Block { index: 10, name: "逐卷大纲".into(), fields: vec!["卷大纲".into(), "新增角色".into(), "叙事意图".into(), "情感节拍".into(), "关键场景".into()], fm: "## 卷大纲:{}\n## 新增角色:{}\n## 叙事意图:{}\n## 情感节拍:{}\n## 关键场景:{}".into() },
    Block { index: 11, name: "逐章大纲".into(), fields: vec!["章节大纲".into()], fm: "## 章节大纲:{}".into() },
    Block { index: 12, name: "正文创作".into(), fields: vec!["正文内容".into()], fm: "## 正文内容:{}".into() },
]);

/// 5 作者 + 主持人（Web 版 AUTHORS/MODERATOR 逐字对齐）
pub static NOVEL_AUTHORS: std::sync::LazyLock<Vec<Author>> = std::sync::LazyLock::new(|| vec![
    Author { name: "司世".into(), zi: "字观止".into(), specialty: "世界观派".into(), description: "负责设定规则/文明结构".into() },
    Author { name: "司人".into(), zi: "字知微".into(), specialty: "人物派".into(), description: "负责角色心理/对话可信度".into() },
    Author { name: "司局".into(), zi: "字守衡".into(), specialty: "结构派".into(), description: "负责情节框架/伏笔节奏".into() },
    Author { name: "司言".into(), zi: "字琢之".into(), specialty: "文笔派".into(), description: "负责语言质感/描写".into() },
    Author { name: "司情".into(), zi: "字动心".into(), specialty: "共情派".into(), description: "负责开场引力/代入感".into() },
]);

pub static NOVEL_MODERATOR: std::sync::LazyLock<Moderator> = std::sync::LazyLock::new(|| Moderator {
    name: "主持人".into(),
        role: "引导讨论、整合意见".into(),
    });

/// 项目模板注册（引擎按 project_type 取——未来剧本/方案 = 新增注册——引擎不改）
#[derive(Debug, Clone)]
pub struct ProjectTemplate {
    pub project_type: &'static str,
    pub blocks: &'static [Block],
    pub authors: &'static [Author],
    pub moderator: &'static Moderator,
}

pub fn novel_template() -> ProjectTemplate {
    ProjectTemplate {
        project_type: "novel",
        blocks: &NOVEL_BLOCKS,
        authors: &NOVEL_AUTHORS,
        moderator: &NOVEL_MODERATOR,
    }
}

/// 取模板（未知类型回退 novel）
pub fn get_template(project_type: &str) -> ProjectTemplate {
    match project_type {
        "novel" => novel_template(),
        _ => novel_template(),
    }
}

/// 按名找 Block
pub fn find_block<'a>(blocks: &'a [Block], name: &str) -> Option<&'a Block> {
    blocks.iter().find(|b| b.name == name)
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
        assert_eq!(find_block(t.blocks, "世界观").unwrap().fields.len(), 5);
    }
}
