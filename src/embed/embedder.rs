//! embed/embedder.rs — 向量嵌入（T5-2——2026-09-03）
//! fastembed 6.x + bge-small-zh-v1.5（512 维——与 Python 版同款）
//! 模型：ONNX 版（本机 PyTorch 缓存不含 onnx——首次运行经 HF 下载——FASTEMBED_CACHE_DIR 可指缓存目录）
//! 单例共享（模型加载 90MB 内存——引擎全局一次）

use std::sync::Mutex;

pub type Embedding = Vec<f32>;

/// 全局嵌入器（懒加载——TextEmbedding 非 Sync——Mutex 串行嵌入）
static EMBEDDER: Mutex<Option<EmbedderState>> = Mutex::new(None);

struct EmbedderState {
    model: Box<dyn EmbedModel>,
}

/// 嵌入模型 trait（解耦 fastembed 具体类型——测试可 mock）
pub trait EmbedModel: Send {
    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Embedding>, String>;
}

/// fastembed 实现
pub struct FastEmbedModel {
    inner: fastembed::TextEmbedding,
}

impl EmbedModel for FastEmbedModel {
    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Embedding>, String> {
        let v = self
            .inner
            .embed(texts, None)
            .map_err(|e| format!("嵌入失败: {e}"))?;
        Ok(v)
    }
}

/// 初始化 fastembed（cache_dir 优先 FASTEMBED_CACHE_DIR——默认 ~/.cache/huggingface/hub）
pub fn init() -> Result<(), String> {
    let mut guard = EMBEDDER.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Ok(());
    }
    let cache = std::env::var("FASTEMBED_CACHE_DIR").unwrap_or_else(|_| {
        std::env::var("HOME").map(|h| format!("{h}/.cache/huggingface/hub")).unwrap_or_else(|_| ".fastembed_cache".into())
    });
    let opts = fastembed::InitOptionsWithLength::new(fastembed::EmbeddingModel::BGESmallZHV15)
        .with_cache_dir(std::path::PathBuf::from(cache))
        .with_show_download_progress(false)
        .with_intra_threads(2);
    let model = fastembed::TextEmbedding::try_new(opts).map_err(|e| format!("fastembed 初始化失败: {e}"))?;
    *guard = Some(EmbedderState { model: Box::new(FastEmbedModel { inner: model }) });
    Ok(())
}

/// 嵌入文本列表（返回归一化向量——未初始化自动自举）
pub fn embed(texts: &[String]) -> Result<Vec<Embedding>, String> {
    let mut guard = EMBEDDER.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        drop(guard);
        init()?;
        guard = EMBEDDER.lock().map_err(|e| e.to_string())?;
    }
    let state = guard.as_mut().ok_or_else(|| "嵌入器未初始化".to_string())?;
    let mut vecs = state.model.embed_texts(texts)?;
    for v in &mut vecs {
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }
    Ok(vecs)
}

/// 余弦相似度（向量已归一化——点积即余弦）
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// 文本间相似度（便捷——用于测试/检索）
pub fn similarity(a: &str, b: &str) -> Result<f32, String> {
    let vecs = embed(&[a.to_string(), b.to_string()])?;
    Ok(cosine(&vecs[0], &vecs[1]))
}

/// 重置（测试用）
pub fn reset() {
    let mut guard = EMBEDDER.lock().unwrap();
    *guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真嵌入：维度 512——相似句 vs 不相似句
    #[test]
    fn real_embed_dim_and_sim() {
        // 需网络/模型文件——本地无 onnx 时可能下载失败——标记环境
        match init() {
            Ok(()) => {}
            Err(e) => {
                eprintln!("[跳过] fastembed 不可用: {e}");
                return;
            }
        }
        let v = embed(&["测试句子".to_string()]).unwrap();
        assert_eq!(v[0].len(), 512, "bge-small-zh 512 维");
        let sim_same = similarity("主角穿越到修仙世界", "少年穿越异界踏上修行之路").unwrap();
        let sim_diff = similarity("主角穿越到修仙世界", "今天天气很好适合散步").unwrap();
        assert!(sim_same > sim_diff, "相似句相关度应更高——same {sim_same} diff {sim_diff}");
        reset();
    }
}
