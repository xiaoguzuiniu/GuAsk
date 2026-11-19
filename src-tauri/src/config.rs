use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Template {
    pub name: String,
    pub prompt: String,
}

fn tpl(name: &str, prompt: &str) -> Template {
    Template { name: name.into(), prompt: prompt.into() }
}

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
    /// 单个英文单词时的词典提示词
    pub word_prompt: String,
    /// 选中代码/报错/日志时的提示词
    pub code_prompt: String,
    /// 自动识别代码/报错
    pub code_detect: bool,
    /// 自定义提示词模板（弹窗顶部切换）
    pub templates: Vec<Template>,
    /// 保存历史记录
    pub history_enabled: bool,
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
            word_prompt: "你是英汉词典。用户给出一个英文单词，请严格按以下 Markdown 格式输出，不要任何多余开场白：\n英 /音标/  美 /音标/\n\n**词性.** {lang}释义1；释义2\n**词性.** {lang}释义\n\n### 例句\n1. 英文例句（把该单词加粗）\n   {lang}翻译\n2. 英文例句\n   {lang}翻译\n\n### 常见搭配\n- 搭配 — 释义".into(),
            code_prompt: "你是资深软件工程师。用户选中了一段代码、报错或日志。请用{lang}：\n- 如果是报错/异常/堆栈：先一句话说明错误含义，再列出最可能的原因和具体的修复方法；\n- 如果是代码：简要说明它做了什么，指出关键逻辑和潜在问题；\n- 如果是日志：解释关键信息并指出异常点。\n保持简洁，必要时给出代码示例。".into(),
            code_detect: true,
            history_enabled: true,
            templates: vec![
                tpl("润色", "你是写作编辑。请润色用户给的文本，使其更通顺、专业、地道，保持原语言和原意，只输出润色后的结果。"),
                tpl("总结", "请用{lang}总结用户给的内容，先一句话概括，再用 3-5 个要点列出关键信息。"),
                tpl("写注释", "你是资深工程师。为用户给的代码补充清晰的注释（注释用{lang}），保持代码逻辑不变，输出完整代码块。"),
                tpl("解释代码", "你是资深工程师。请用{lang}逐段解释用户给的代码在做什么，说明关键逻辑、用到的 API 和可能的坑。"),
            ],
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
