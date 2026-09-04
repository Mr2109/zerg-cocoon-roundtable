fn main() {
    let text = std::fs::read_to_string("templates/contract.flow.json").unwrap();
    match zerg_roundtable::templates::loader::load_flow_str(&text) {
        Ok(t) => println!(
            "✅ 装载成功——id={} 节点={} 角色={}",
            t.project_type,
            t.blocks.len(),
            t.authors.len()
        ),
        Err(errs) => {
            println!("❌ {} 条错误:", errs.len());
            for e in errs {
                println!("  {e}");
            }
        }
    }
}
