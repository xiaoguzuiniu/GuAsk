use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 翻译固定使用的模型（空则用 model）
    pub translate_model: String,
    /// 提问时可在弹窗中切换的模型
    pub ask_models: Vec<String>,
    /// 提问上次使用的模型
    pub ask_model: String,
    pub target_lang: String,
    /// 划词后自动翻译
    pub auto_translate: bool,
    pub translate_prompt: String,
    pub ask_prompt: String,
    /// 唤出方式：ctrl2 / alt2 / shift2 / auto
    pub trigger: String,
    /// 全局快捷键：直接打开提问（空则禁用）
    pub hotkey: String,
    /// 弹窗大小（逻辑像素）
    pub popup_w: f64,
    pub popup_h: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: String::new(),
            model: "deepseek-chat".into(),
            translate_model: String::new(),
            ask_models: Vec::new(),
            ask_model: String::new(),
            target_lang: "简体中文".into(),
            auto_translate: true,
            translate_prompt: "你是专业翻译。把用户给的文本翻译成{lang}。如果是单个单词或短语，给出音标、词性、常用释义和1-2个例句；如果是句子或段落，只输出地道译文。".into(),
            ask_prompt: "你是知识渊博的助手。用户在阅读时选中了一段内容，请用{lang}简洁、准确地解释或回答。".into(),
            trigger: "ctrl2".into(),
            hotkey: "Alt+Q".into(),
            popup_w: 440.0,
            popup_h: 380.0,
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
