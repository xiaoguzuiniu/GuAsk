use crate::config::Config;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::ipc::Channel;

#[derive(Clone, Serialize, Deserialize)]
pub struct Msg {
    pub role: String,
    pub content: String,
}

/// 调用 OpenAI 兼容接口，流式把增量文本推给前端
pub async fn chat(cfg: Config, messages: Vec<Msg>, ch: Channel<String>) -> Result<(), String> {
    if cfg.api_key.trim().is_empty() {
        return Err("未配置 API Key，请右键托盘图标 → 设置".into());
    }
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(cfg.api_key.trim())
        .json(&json!({ "model": cfg.model, "messages": messages, "stream": true }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let st = resp.status();
        return Err(format!("{}: {}", st, resp.text().await.unwrap_or_default()));
    }
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
        while let Some(i) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=i).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim().strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                return Ok(());
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(s) = v["choices"][0]["delta"]["content"].as_str() {
                    let _ = ch.send(s.to_string());
                }
            }
        }
    }
    Ok(())
}
