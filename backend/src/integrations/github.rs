//! GitHub for projects with a `repo` property: recent commits and open issues as a digest
//! the inhabitants can comment on. Public repos need no token; `GITHUB_TOKEN` raises limits
//! and opens private ones.

use anyhow::{Context, Result};

use super::http;
use crate::{config::LlmSettings, mermaid};

fn normalize(repo: &str) -> Option<String> {
    let r = repo.trim().trim_end_matches('/').trim_end_matches(".git");
    let r = r.strip_prefix("https://github.com/").or_else(|| r.strip_prefix("http://github.com/")).or_else(|| r.strip_prefix("github.com/")).unwrap_or(r);
    let mut parts = r.split('/');
    let (o, n) = (parts.next()?, parts.next()?);
    if o.is_empty() || n.is_empty() {
        return None;
    }
    Some(format!("{o}/{n}"))
}

async fn get(s: &LlmSettings, url: &str) -> Result<serde_json::Value> {
    let mut req = http().get(url).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(t) = s.get("GITHUB_TOKEN") {
        req = req.bearer_auth(t);
    }
    Ok(req.send().await?.error_for_status()?.json().await?)
}

/// Commits (5) + open issues/PRs (5) + stars, as prompt text.
pub async fn activity(s: &LlmSettings, repo: &str) -> Result<String> {
    let repo = normalize(repo).context("repo должен быть вида owner/name")?;
    let info = get(s, &format!("https://api.github.com/repos/{repo}")).await?;
    let commits = get(s, &format!("https://api.github.com/repos/{repo}/commits?per_page=5")).await.unwrap_or_default();
    let issues = get(s, &format!("https://api.github.com/repos/{repo}/issues?state=open&per_page=5")).await.unwrap_or_default();
    let mut out = format!(
        "{repo} — ★{} · {} · обновлён {}{}\n",
        info["stargazers_count"].as_i64().unwrap_or(0),
        info["language"].as_str().unwrap_or("?"),
        info["pushed_at"].as_str().map(|d| d.get(0..10).unwrap_or(d)).unwrap_or("?"),
        info["description"].as_str().map(|d| format!(" — {}", mermaid::truncate_chars(d, 120))).unwrap_or_default()
    );
    if let Some(arr) = commits.as_array() {
        out.push_str("Коммиты:\n");
        for c in arr {
            let msg = c["commit"]["message"].as_str().unwrap_or("").lines().next().unwrap_or("");
            out.push_str(&format!("- {} {} ({})\n", c["commit"]["author"]["date"].as_str().map(|d| d.get(0..10).unwrap_or(d)).unwrap_or(""), mermaid::truncate_chars(msg, 90), c["commit"]["author"]["name"].as_str().unwrap_or("")));
        }
    }
    if let Some(arr) = issues.as_array().filter(|a| !a.is_empty()) {
        out.push_str("Открытые issues/PR:\n");
        for i in arr {
            out.push_str(&format!("- #{} {}{}\n", i["number"].as_i64().unwrap_or(0), mermaid::truncate_chars(i["title"].as_str().unwrap_or(""), 90), if i.get("pull_request").is_some() { " (PR)" } else { "" }));
        }
    }
    Ok(out.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_repo_forms() {
        assert_eq!(normalize("https://github.com/a/b.git/").as_deref(), Some("a/b"));
        assert_eq!(normalize("a/b").as_deref(), Some("a/b"));
        assert_eq!(normalize("nope"), None);
    }
}
