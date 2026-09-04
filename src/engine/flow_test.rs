//! engine/flow_test.rs — B1 集成验收：三节点流（discussion→single→gate）mock 跑通
//! 用 flow.json 声明 + loader 装载 + RtFlowNode 执行——端到端走一遍节点层

#[cfg(test)]
mod tests {
    use crate::ai::mock::MockProvider;
    use crate::ai::{AiMessage, AiProvider};
    use crate::db::pool::Db;
    use crate::engine::discussion::DiscussionState;
    use crate::engine::nodes::{GateNode, NodeCtx, RtFlowNode, SingleNode};
    use crate::templates::loader::load_flow_str;

    const THREE_NODE_FLOW: &str = r##"{
        "id": "demo3", "name": "三节点演示", "version": 1,
        "inputs": [{"key": "topic", "label": "主题", "required": true}],
        "roles": {
            "moderator": {"name": "主持人", "role": "引导"},
            "panel": [
                {"name": "甲", "zi": "字一", "specialty": "分析", "description": "负责分析"},
                {"name": "乙", "zi": "字二", "specialty": "执行", "description": "负责执行"}
            ]
        },
        "nodes": [
            {"id": "n0", "kind": "single", "name": "抽取", "desc": "从主题{{input.topic}}提取核心关键词，输出格式：关键词: xxx",
             "fields": ["关键词"], "fm": "= 关键词:{}"},
            {"id": "n1", "kind": "single", "name": "总结", "desc": "总结{{n0.关键词}}",
             "fields": ["摘要"], "fm": "= 摘要:{}", "next": ["n2"]},
            {"id": "n2", "kind": "gate", "name": "验收", "desc": "check:{{n1.摘要}} 包含 复仇"}
        ],
        "gate": {"pass_score": 80, "max_cycle": 3}
    }"##;

    /// 最小 mock AI（复用 MockProvider——按响应序列出）
    fn mock_ai(script: Vec<String>) -> Box<dyn AiProvider> {
        let refs: Vec<&str> = script.iter().map(|s| s.as_str()).collect();
        Box::new(MockProvider::new(refs))
    }

    #[tokio::test]
    async fn three_node_flow_single_gate() {
        let _ = std::fs::remove_file("/tmp/yz_flow3.db");
        let db = Db::open("/tmp/yz_flow3.db").await.unwrap();
        db.create_session("f3", "仙侠复仇记", "玄幻", "长篇", "zerg", "demo3")
            .await
            .unwrap();

        let tmpl = load_flow_str(THREE_NODE_FLOW).expect("三节点流装载");
        assert_eq!(tmpl.blocks.len(), 3);

        // mock 响应：n0 single 抽取 → n1 single 总结 → n2 gate（无 AI 调用）
        let ai = mock_ai(vec![
            "关键词: 复仇".to_string(),
            "摘要: 这是一个关于复仇的故事，主角为复仇而修行。".to_string(),
        ]);

        let mut state = DiscussionState::new("f3", "仙侠复仇记", "玄幻", "长篇", "zerg")
            .with_input_vars(std::collections::HashMap::from([(
                "input.topic".to_string(),
                "仙侠复仇记".to_string(),
            )]));

        let stop = std::sync::atomic::AtomicBool::new(false);
        let _ = stop; // single/gate 不查 stop（快操作）——B2 主循环统一查

        let mut vars = std::collections::HashMap::new();
        vars.insert("input.topic".to_string(), "仙侠复仇记".to_string());

        // n0 single
        {
            let ai_ref: &dyn AiProvider = ai.as_ref();
            let mut ctx = NodeCtx {
                db: &db,
                ai: ai_ref,
                state: &mut state,
                vars: vars.clone(),
            };
            let out = SingleNode.execute(&mut ctx, &tmpl.blocks[0]).await.unwrap();
            assert!(out.locked);
            assert!(
                out.produced.iter().any(|(k, _)| k == "n0.关键词"),
                "n0 产出入变量池: {:?}",
                out.produced
            );
        }
        // n1 single（引用 {{n0.关键词}}——先刷新 vars）
        {
            for (nid, k, v) in db.get_flow_vars("f3").await.unwrap() {
                vars.insert(format!("{nid}.{k}"), v);
            }
            let ai_ref: &dyn AiProvider = ai.as_ref();
            let mut ctx = NodeCtx {
                db: &db,
                ai: ai_ref,
                state: &mut state,
                vars: vars.clone(),
            };
            let out = SingleNode.execute(&mut ctx, &tmpl.blocks[1]).await.unwrap();
            assert!(out.locked);
        }
        // n2 gate（check:{{n1.摘要}} 包含 复仇——摘要含"复仇"→通过）
        {
            for (nid, k, v) in db.get_flow_vars("f3").await.unwrap() {
                vars.insert(format!("{nid}.{k}"), v);
            }
            let ai_ref: &dyn AiProvider = ai.as_ref();
            let mut ctx = NodeCtx {
                db: &db,
                ai: ai_ref,
                state: &mut state,
                vars: vars.clone(),
            };
            let out = GateNode.execute(&mut ctx, &tmpl.blocks[2]).await.unwrap();
            assert!(
                out.locked,
                "gate 应通过（摘要含'复仇'）——reason: {}",
                out.reason
            );
            assert!(out
                .produced
                .iter()
                .any(|(k, v)| k == "n2.判定" && v == "通过"));
        }
        // 变量池终态
        let all = db.get_flow_vars("f3").await.unwrap();
        assert!(all.iter().any(|(n, k, _)| n == "n0" && k == "关键词"));
        assert!(all.iter().any(|(n, k, _)| n == "n2" && k == "判定"));
        std::fs::remove_file("/tmp/yz_flow3.db").ok();
    }

    #[tokio::test]
    async fn gate_fails_on_miss() {
        let _ = std::fs::remove_file("/tmp/yz_flow3b.db");
        let db = Db::open("/tmp/yz_flow3b.db").await.unwrap();
        db.create_session("f3b", "都市温情", "都市", "短篇", "zerg", "demo3")
            .await
            .unwrap();
        let tmpl = load_flow_str(THREE_NODE_FLOW).unwrap();
        // 摘要不含"复仇"→gate 不通过
        let ai = mock_ai(vec![
            "关键词: 温情".to_string(),
            "摘要: 这是一个关于陪伴与成长的温暖故事。".to_string(),
        ]);
        let mut state = DiscussionState::new("f3b", "都市温情", "都市", "短篇", "zerg");
        let mut vars = std::collections::HashMap::new();
        vars.insert("input.topic".to_string(), "都市温情".to_string());
        // n0/n1 快速填变量
        db.set_flow_var("f3b", "n0", "关键词", "温情")
            .await
            .unwrap();
        db.set_flow_var("f3b", "n1", "摘要", "这是一个关于陪伴与成长的温暖故事。")
            .await
            .unwrap();
        for (nid, k, v) in db.get_flow_vars("f3b").await.unwrap() {
            vars.insert(format!("{nid}.{k}"), v);
        }
        let ai_ref: &dyn AiProvider = ai.as_ref();
        let mut ctx = NodeCtx {
            db: &db,
            ai: ai_ref,
            state: &mut state,
            vars,
        };
        let out = GateNode.execute(&mut ctx, &tmpl.blocks[2]).await.unwrap();
        assert!(!out.locked, "摘要不含'复仇'——gate 应不通过");
        std::fs::remove_file("/tmp/yz_flow3b.db").ok();
    }

    #[test]
    fn next_declaration_reorders() {
        // 构造：3 块，n0.next=["n2"]——从 n0 跑完应跳到 n2（跳过 n1）——用 run 的推进公式直接验
        let flow = r##"{
            "id": "skip", "name": "跳转", "version": 1, "inputs": [],
            "roles": {"moderator": {"name": "主持", "role": "r"}, "panel": [
                {"name": "甲", "zi": "字一", "specialty": "s", "description": "d"}
            ]},
            "nodes": [
                {"id": "n0", "kind": "gate", "name": "入口", "desc": ""},
                {"id": "n1", "kind": "gate", "name": "被跳过", "desc": ""},
                {"id": "n2", "kind": "gate", "name": "终点", "desc": ""}
            ]
        }"##;
        let flow_v2 = flow.replace(
            r#"{"id": "n0", "kind": "gate", "name": "入口", "desc": ""}"#,
            r#"{"id": "n0", "kind": "gate", "name": "入口", "desc": "", "next": ["n2"]}"#,
        );
        let t = load_flow_str(&flow_v2).unwrap();
        // 模拟 run_discussion 推进公式：next[0] 解析到目标索引
        let blk = &t.blocks[0];
        assert_eq!(blk.next, vec!["n2"]);
        let next_idx = t
            .blocks
            .iter()
            .position(|b| blk.next[0] == format!("n{}", b.index) || blk.next[0] == b.name);
        assert_eq!(next_idx, Some(2), "n0.next=n2 应解析到索引 2——跳过 n1");
    }

    #[tokio::test]
    async fn human_confirm_pauses_then_resumes() {
        // B3 端到端：human_gate=every_step 的 gate——首次执行挂起（awaiting_human）→人工裁决→gate 按裁决出
        use crate::templates::Block;
        let _ = std::fs::remove_file("/tmp/yz_flow3c.db");
        let db = Db::open("/tmp/yz_flow3c.db").await.unwrap();
        db.create_session("f3c", "测试确认", "都市", "短篇", "zerg", "demo3")
            .await
            .unwrap();
        let mut state = DiscussionState::new("f3c", "测试确认", "都市", "短篇", "zerg");
        let ai = mock_ai(vec![]);
        let ai_ref: &dyn AiProvider = ai.as_ref();
        let blk = Block {
            index: 2,
            name: "终审".into(),
            fields: vec![],
            fm: String::new(),
            kind: "gate".into(),
            desc: "请确认方案".into(),
            next: vec![],
            human_gate: "every_step".into(), // 关键：非 none
            model: String::new(),
        };
        let mut vars = std::collections::HashMap::new();
        // 首次：无裁决——应挂起
        {
            let mut ctx = NodeCtx {
                db: &db,
                ai: ai_ref,
                state: &mut state,
                vars: vars.clone(),
            };
            let out = GateNode.execute(&mut ctx, &blk).await.unwrap();
            assert!(!out.locked, "无裁决应挂起");
            assert!(out.reason.contains("awaiting_human"));
        }
        let s = db.get_session("f3c").await.unwrap().unwrap();
        assert_eq!(s.status, "awaiting_human", "会话状态应置 awaiting_human");
        // 人工批准（模拟 UI human_decide）
        db.set_flow_var("f3c", "n2", "人工裁决", "通过")
            .await
            .unwrap();
        db.update_session_progress("f3c", 2, "idle").await.unwrap();
        vars.insert("n2.人工裁决".to_string(), "通过".to_string());
        // 续跑：有裁决——应通过
        let mut state2 = DiscussionState::new("f3c", "测试确认", "都市", "短篇", "zerg");
        {
            let mut ctx = NodeCtx {
                db: &db,
                ai: ai_ref,
                state: &mut state2,
                vars,
            };
            let out = GateNode.execute(&mut ctx, &blk).await.unwrap();
            assert!(
                out.locked,
                "人工'通过'后 gate 应通过——reason: {}",
                out.reason
            );
            assert!(out
                .produced
                .iter()
                .any(|(k, v)| k == "n2.判定" && v == "通过"));
        }
        std::fs::remove_file("/tmp/yz_flow3c.db").ok();
    }
}
