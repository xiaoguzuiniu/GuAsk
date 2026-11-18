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

fn client() -> reqwest::Client {
    reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build().unwrap()
}

fn base(cfg: &Config) -> String {
    cfg.base_url.trim().trim_end_matches('/').to_string()
}

/// GET /models，返回排序后的模型 id 列表
pub async fn list_models(cfg: Config) -> Result<Vec<String>, String> {
    let resp = client()
        .get(format!("{}/models", base(&cfg)))
        .bearer_auth(cfg.api_key.trim())
        .send()
        .await
        .map_err(|e| format!("连接失败：{e}"))?;
    let st = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !st.is_success() {
        return Err(format!("{st}: {}", body.chars().take(300).collect::<String>()));
    }
    let v: Value = serde_json::from_str(&body).map_err(|_| "返回不是 JSON，Base URL 可能不对".to_string())?;
    let mut ids: Vec<String> = v["data"]
        .as_array()
        .or_else(|| v["models"].as_array())
        .map(|a| a.iter().filter_map(|m| m["id"].as_str().or(m["name"].as_str()).map(String::from)).collect())
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// 发一条极短的非流式请求，返回 (耗时ms, 模型回复)
pub async fn test(cfg: Config) -> Result<(u128, String), String> {
    let t = std::time::Instant::now();
    let resp = client()
        .post(format!("{}/chat/completions", base(&cfg)))
        .bearer_auth(cfg.api_key.trim())
        .json(&json!({ "model": cfg.model, "messages": [{"role":"user","content":"Reply with: OK"}], "max_tokens": 8, "stream": false }))
        .send()
        .await
        .map_err(|e| format!("连接失败：{e}"))?;
    let st = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !st.is_success() {
        return Err(format!("{st}: {}", body.chars().take(300).collect::<String>()));
    }
    let v: Value = serde_json::from_str(&body).unwrap_or_default();
    let reply = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
    Ok((t.elapsed().as_millis(), reply))
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
