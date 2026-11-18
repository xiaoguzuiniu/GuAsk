use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub target_lang: String,
    /// 划词后自动翻译
    pub auto_translate: bool,
    pub translate_prompt: String,
    pub ask_prompt: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: String::new(),
            model: "deepseek-chat".into(),
            target_lang: "简体中文".into(),
            auto_translate: true,
            translate_prompt: "你是专业翻译。把用户给的文本翻译成{lang}。如果是单个单词或短语，给出音标、词性、常用释义和1-2个例句；如果是句子或段落，只输出地道译文。".into(),
            ask_prompt: "你是知识渊博的助手。用户在阅读时选中了一段内容，请用{lang}简洁、准确地解释或回答。".into(),
        }
    }
}

pub fn path(dir: PathBuf) -> PathBuf {
    dir.join("config.json")
}

pub fn load(dir: PathBuf) -> Config {
    std::fs::read_to_string(path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(dir: PathBuf, cfg: &Config) -> Result<(), String> {
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let s = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(path(dir), s).map_err(|e| e.to_string())
}
