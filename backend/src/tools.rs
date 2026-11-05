//! The room as an MCP-style tool surface. One registry serves three consumers: the
//! inhabitants (native tool calling), the `<act>` tag fallback for models without it, and
//! external MCP clients over `/mcp`. Every tool has a JSON schema and a plain-language
//! description so an agent can discover and *use* the system instead of being told about it.

use anyhow::Result;
use serde_json::{json, Value};

use crate::{db, mermaid, models::ActionRecord, state::AppState};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "library_search".into(),
            description: "Найти объекты в библиотеке комнаты (люди, проекты, задачи, идеи, закладки, книги…) по словам и/или типу. Возвращает id, тип, заголовок, свойства, тело. Используй, когда нужны факты о делах игрока или свои прошлые записи.".into(),
            input_schema: obj(json!({ "query": { "type": "string", "description": "ключевые слова" }, "type": { "type": "string", "description": "id типа (task, human, project, idea, bookmark, book, …) или пусто" }, "limit": { "type": "integer", "minimum": 1, "maximum": 20 } }), &[]),
        },
        ToolDef {
            name: "library_get".into(),
            description: "Полная карточка объекта по id: свойства, тело, связи, пометки, где упоминался.".into(),
            input_schema: obj(json!({ "id": { "type": "string" } }), &["id"]),
        },
        ToolDef {
            name: "library_create".into(),
            description: "Завести объект: задачу игроку или себе, человека, идею, проект, закладку, цель… Свойства — по схеме типа (см. library_types). Автор — ты.".into(),
            input_schema: obj(json!({ "type": { "type": "string" }, "title": { "type": "string" }, "body": { "type": "string" }, "props": { "type": "object" } }), &["type", "title"]),
        },
        ToolDef {
            name: "library_update".into(),
            description: "Изменить объект: заголовок, тело, тип, свойства (например done=true, status, due). Свойства сливаются, null удаляет.".into(),
            input_schema: obj(json!({ "id": { "type": "string" }, "title": { "type": "string" }, "body": { "type": "string" }, "type": { "type": "string" }, "props": { "type": "object" } }), &["id"]),
        },
        ToolDef {
            name: "library_append".into(),
            description: "Дописать в тело заметки/объекта, не стирая написанное: абзац, пункт списка, запись в лог. Можно указать heading — раздел «## …», куда дописать. Для своих дневников, логов проектов, развития чужой идеи.".into(),
            input_schema: obj(json!({ "id": { "type": "string" }, "text": { "type": "string", "description": "markdown" }, "heading": { "type": "string" } }), &["id", "text"]),
        },
        ToolDef {
            name: "library_link".into(),
            description: "Связать два объекта: kind — belongs_to (задача→проект), about, blocks, assigned_to (задача→человек), contradicts, related.".into(),
            input_schema: obj(json!({ "from": { "type": "string" }, "to": { "type": "string" }, "kind": { "type": "string" }, "label": { "type": "string" } }), &["from", "to"]),
        },
        ToolDef {
            name: "library_note".into(),
            description: "Оставить свою пометку на объекте — твоё мнение, которое останется в его карточке.".into(),
            input_schema: obj(json!({ "id": { "type": "string" }, "text": { "type": "string" } }), &["id", "text"]),
        },
        ToolDef {
            name: "library_types".into(),
            description: "Список типов объектов и их свойств (схемы).".into(),
            input_schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "contracts_list".into(),
            description: "Открытые слова игрока (обещания с дедлайнами и поручителями) и недавняя история.".into(),
            input_schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "room_status".into(),
            description: "Кто в комнате, их состояние и отношения, текущее время (UTC), сколько игрок молчит, погода за окном.".into(),
            input_schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "file_read".into(),
            description: "Прочитать текст приложенного к объекту файла (pdf, txt, md, csv…) по id вложения из library_get. Кусками по 4000 символов: offset — с какого символа.".into(),
            input_schema: obj(json!({ "id": { "type": "string" }, "offset": { "type": "integer", "minimum": 0 } }), &["id"]),
        },
        ToolDef {
            name: "web_search".into(),
            description: "Поиск в интернете: заголовок, ссылка, фрагмент. Для фактов, новостей, цен, дат, цитат — всего, чего нет в библиотеке. Потом можно web_fetch.".into(),
            input_schema: obj(json!({ "query": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 10 } }), &["query"]),
        },
        ToolDef {
            name: "web_fetch".into(),
            description: "Открыть страницу по URL и получить её текст (до 6000 символов).".into(),
            input_schema: obj(json!({ "url": { "type": "string" } }), &["url"]),
        },
        ToolDef {
            name: "lookup".into(),
            description: "Справочник: kind=book (OpenLibrary: автор, год, страницы, обложка), movie (TMDB: год, режиссёр, постер, описание), wiki (Википедия: люди, места, понятия). Возвращает карточку; чтобы записать — library_create/update с этими свойствами.".into(),
            input_schema: obj(json!({ "kind": { "type": "string", "enum": ["book", "movie", "wiki"] }, "query": { "type": "string" } }), &["kind", "query"]),
        },
        ToolDef {
            name: "weather_now".into(),
            description: "Погода за окном сейчас и на завтра (город из настроек).".into(),
            input_schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "calendar_upcoming".into(),
            description: "Что у игрока в календаре на ближайшие дни: сроки задач и слов из библиотеки плюс его личный календарь.".into(),
            input_schema: obj(json!({ "days": { "type": "integer", "minimum": 1, "maximum": 60 } }), &[]),
        },
        ToolDef {
            name: "feed_fetch".into(),
            description: "Прочитать RSS/Atom-ленту по URL: свежие заголовки с ссылками и выжимками.".into(),
            input_schema: obj(json!({ "url": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 20 } }), &["url"]),
        },
        ToolDef {
            name: "github_activity".into(),
            description: "Что происходит в GitHub-репозитории (owner/name или ссылка): последние коммиты, открытые issues, звёзды.".into(),
            input_schema: obj(json!({ "repo": { "type": "string" } }), &["repo"]),
        },
    ]
}

/// Tools that are actually usable with the current settings (so the model never sees a dead tool).
pub fn definitions_enabled(s: &crate::config::LlmSettings) -> Vec<ToolDef> {
    let f = crate::integrations::features(s);
    definitions()
        .into_iter()
        .filter(|t| match t.name.as_str() {
            "web_search" | "web_fetch" => f.search,
            "weather_now" => f.weather,
            _ => true,
        })
        .collect()
}

/// OpenAI/OpenRouter `tools` payload.
pub fn as_openai_tools(s: &crate::config::LlmSettings) -> Vec<Value> {
    definitions_enabled(s)
        .into_iter()
        .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.input_schema } }))
        .collect()
}

/// Execute a tool on behalf of `actor` (an agent id, or "user"/"mcp"). Returns the JSON result
/// for the model and, for mutations, a human-readable record for the chat.
pub async fn call(app: &AppState, actor: &str, name: &str, args: &Value) -> Result<(Value, Option<ActionRecord>)> {
    let types = db::list_types(&app.db).await?;
    let agents = db::list_agents(&app.db).await?;
    match name {
        "library_types" => Ok((json!({ "types": types.iter().map(|t| json!({ "id": t.id, "name": t.name, "props": t.schema.0.get("props") })).collect::<Vec<_>>() }), None)),
        "library_search" => {
            let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let ty = args.get("type").and_then(|v| v.as_str()).filter(|t| !t.is_empty());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(8).min(20) as usize;
            let mut hits = if q.trim().is_empty() && ty.is_none() {
                db::list_nodes(&app.db).await?.into_iter().rev().take(limit).collect()
            } else {
                db::search_objects(&app.db, q, ty, limit).await?
            };
            let mut via_files: Vec<Value> = vec![];
            if !q.trim().is_empty() {
                for (node_id, fname) in db::search_attachments(&app.db, q, 5).await? {
                    if ty.map(|t| hits.iter().any(|h| h.id == node_id && h.type_id == t)).unwrap_or(true) && !hits.iter().any(|h| h.id == node_id) {
                        if let Some(n) = db::get_node(&app.db, &node_id).await? {
                            if ty.map(|t| n.type_id == t).unwrap_or(true) {
                                via_files.push(json!({ "object": short(&n, &agents), "matched_file": fname }));
                                hits.push(n);
                            }
                        }
                    }
                }
            }
            Ok((json!({ "results": hits.iter().map(|n| short(n, &agents)).collect::<Vec<_>>(), "matched_in_files": via_files }), None))
        }
        "library_get" => {
            let id = mermaid::safe_id(args.get("id").and_then(|v| v.as_str()).unwrap_or(""));
            let Some(n) = db::get_node(&app.db, &id).await? else { return Ok((json!({ "error": "нет такого объекта" }), None)) };
            let edges = db::edges_of(&app.db, &id).await?;
            let notes = db::notes_for_node(&app.db, &id).await?;
            let mentions = db::messages_mentioning(&app.db, &id, 8).await?;
            let atts = db::attachments_of(&app.db, &id).await?;
            Ok((
                json!({
                    "object": short(&n, &agents),
                    "body": n.body,
                    "attachments": atts.iter().map(|a| json!({ "id": a.id, "name": a.name, "mime": a.mime, "size": a.size, "text_chars": a.text_len, "excerpt": mermaid::truncate_chars(&a.text.split_whitespace().collect::<Vec<_>>().join(" "), 600) })).collect::<Vec<_>>(),
                    "relations": edges.iter().map(|e| json!({ "from": e.source_id, "to": e.target_id, "kind": e.kind, "label": e.relation_label })).collect::<Vec<_>>(),
                    "notes": notes.iter().map(|x| json!({ "author": x.author, "text": x.text })).collect::<Vec<_>>(),
                    "mentioned_in": mentions.iter().map(|m| json!({ "who": crate::prompts::sender_name(&m.sender_id, &agents), "text": mermaid::truncate_chars(&m.content, 160), "at": m.created_at })).collect::<Vec<_>>(),
                }),
                None,
            ))
        }
        "library_create" => {
            let title = args.get("title").and_then(|v| v.as_str()).map(mermaid::sanitize_label).filter(|t| t != "…");
            let Some(title) = title else { return Ok((json!({ "error": "нужен title" }), None)) };
            let type_id = args.get("type").and_then(|v| v.as_str()).map(|t| t.trim().to_lowercase()).filter(|t| types.iter().any(|x| &x.id == t)).unwrap_or_else(|| "note".into());
            let mut id = mermaid::safe_id(&title);
            if db::node_exists(&app.db, &id).await? {
                id = format!("{}_{}", id, chrono::Utc::now().timestamp() % 10_000);
            }
            let body = args.get("body").and_then(|b| b.as_str()).map(|b| mermaid::truncate_chars(b.trim(), 6000));
            let props = filtered_props(&types, &type_id, args.get("props"));
            db::upsert_object(&app.db, &id, &title, &type_id, actor, body.as_deref(), props.as_ref(), None, None).await?;
            db::place_unpositioned_nodes(&app.db).await?;
            let tname = type_name_acc(&types, &type_id);
            Ok((json!({ "ok": true, "id": id, "type": type_id }), Some(ActionRecord { op: "create".into(), object_id: id.clone(), text: format!("завёл {tname} «{title}»") })))
        }
        "library_update" => {
            let id = mermaid::safe_id(args.get("id").and_then(|v| v.as_str()).unwrap_or(""));
            let Some(existing) = db::get_node(&app.db, &id).await? else { return Ok((json!({ "error": "нет такого объекта" }), None)) };
            let label = args.get("title").and_then(|t| t.as_str()).map(mermaid::sanitize_label).filter(|t| t != "…");
            let body = args.get("body").and_then(|b| b.as_str()).map(|b| mermaid::truncate_chars(b.trim(), 6000));
            let type_id = args.get("type").and_then(|t| t.as_str()).map(|t| t.trim().to_lowercase()).filter(|t| types.iter().any(|x| &x.id == t));
            let props = args.get("props").cloned().filter(|p| p.is_object());
            if label.is_some() || body.is_some() {
                db::update_node_fields(&app.db, &id, label.as_deref(), body.as_deref(), None, None).await?;
            }
            if type_id.is_some() || props.is_some() {
                db::update_object_meta(&app.db, &id, type_id.as_deref(), props.as_ref(), None).await?;
            }
            let what = if props.as_ref().and_then(|p| p.get("done")).and_then(|d| d.as_bool()) == Some(true) {
                format!("закрыл «{}»", existing.label)
            } else if let Some(p) = props.as_ref().and_then(|p| p.as_object()).filter(|o| !o.is_empty()) {
                format!("обновил «{}»: {}", existing.label, p.iter().map(|(k, v)| format!("{k}={}", v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect::<Vec<_>>().join(", "))
            } else {
                format!("поправил «{}»", existing.label)
            };
            Ok((json!({ "ok": true, "id": id }), Some(ActionRecord { op: "update".into(), object_id: id, text: what })))
        }
        "library_append" => {
            let id = mermaid::safe_id(args.get("id").and_then(|v| v.as_str()).unwrap_or(""));
            let Some(existing) = db::get_node(&app.db, &id).await? else { return Ok((json!({ "error": "нет такого объекта" }), None)) };
            let text = args.get("text").and_then(|t| t.as_str()).map(|t| mermaid::truncate_chars(t.trim(), 3000)).filter(|t| !t.is_empty());
            let Some(text) = text else { return Ok((json!({ "error": "нужен text" }), None)) };
            let heading = args.get("heading").and_then(|h| h.as_str()).map(|h| mermaid::truncate_chars(h.trim(), 60)).filter(|h| !h.is_empty());
            let body = crate::markdown::append_section(&existing.body, heading.as_deref(), &text);
            db::update_node_fields(&app.db, &id, None, Some(&mermaid::truncate_chars(&body, 60_000)), None, None).await?;
            Ok((json!({ "ok": true, "id": id, "chars": body.chars().count() }), Some(ActionRecord { op: "append".into(), object_id: id, text: format!("дописал в «{}»", existing.label) })))
        }
        "library_link" => {
            let from = mermaid::safe_id(args.get("from").and_then(|v| v.as_str()).unwrap_or(""));
            let to = mermaid::safe_id(args.get("to").and_then(|v| v.as_str()).unwrap_or(""));
            if from == to || !db::node_exists(&app.db, &from).await? || !db::node_exists(&app.db, &to).await? {
                return Ok((json!({ "error": "нужны два разных существующих id" }), None));
            }
            let kind = args.get("kind").and_then(|k| k.as_str()).map(|k| k.trim().to_lowercase()).filter(|k| ["related", "belongs_to", "about", "blocks", "made_by", "assigned_to", "contradicts", "mentions"].contains(&k.as_str())).unwrap_or_else(|| "related".into());
            let label = args.get("label").and_then(|l| l.as_str()).map(mermaid::sanitize_label).filter(|l| l != "…").map(|l| mermaid::truncate_chars(&l, 30)).unwrap_or_default();
            db::insert_relation(&app.db, &from, &to, &kind, &label, actor, 0.9).await?;
            let fl = db::get_node(&app.db, &from).await?.map(|n| n.label).unwrap_or(from.clone());
            let tl = db::get_node(&app.db, &to).await?.map(|n| n.label).unwrap_or(to.clone());
            Ok((json!({ "ok": true }), Some(ActionRecord { op: "link".into(), object_id: from, text: format!("связал «{fl}» → «{tl}»") })))
        }
        "library_note" => {
            let id = mermaid::safe_id(args.get("id").and_then(|v| v.as_str()).unwrap_or(""));
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
            if text.is_empty() || !db::node_exists(&app.db, &id).await? {
                return Ok((json!({ "error": "нужны id существующего объекта и text" }), None));
            }
            db::insert_note(&app.db, &id, actor, &mermaid::truncate_chars(text, 600)).await?;
            let label = db::get_node(&app.db, &id).await?.map(|n| n.label).unwrap_or(id.clone());
            Ok((json!({ "ok": true }), Some(ActionRecord { op: "note".into(), object_id: id, text: format!("оставил пометку на «{label}»") })))
        }
        "contracts_list" => {
            let all = db::list_contracts(&app.db).await?;
            Ok((json!({ "contracts": all.iter().take(20).map(|c| json!({ "id": c.id, "title": c.title, "status": c.status, "deadline": c.deadline, "guarantor": c.guarantor_id, "stake": c.stake })).collect::<Vec<_>>() }), None))
        }
        "file_read" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
            let Some(a) = db::get_attachment(&app.db, id).await? else { return Ok((json!({ "error": "нет такого вложения" }), None)) };
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let total = a.text.chars().count();
            let chunk: String = a.text.chars().skip(offset).take(4000).collect();
            Ok((json!({ "name": a.name, "object_id": a.node_id, "total_chars": total, "offset": offset, "next_offset": if offset + 4000 < total { Some(offset + 4000) } else { None }, "text": if chunk.is_empty() && total == 0 { "(текст не извлечён — это картинка или бинарный файл)".to_string() } else { chunk } }), None))
        }
        "web_search" => {
            let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("").trim();
            if q.is_empty() {
                return Ok((json!({ "error": "нужен query" }), None));
            }
            let s = app.llm_settings().await;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            match crate::integrations::search::search(&s, q, limit).await {
                Ok(hits) => Ok((json!({ "results": hits }), Some(ActionRecord { op: "search".into(), object_id: String::new(), text: format!("искал в сети: {}", mermaid::truncate_chars(q, 60)) }))),
                Err(e) => Ok((json!({ "error": format!("поиск не удался: {e:#}") }), None)),
            }
        }
        "web_fetch" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").trim();
            match crate::integrations::search::fetch(url, 6000).await {
                Ok((title, text)) => Ok((json!({ "title": title, "text": text }), Some(ActionRecord { op: "search".into(), object_id: String::new(), text: format!("прочитал {}", mermaid::truncate_chars(&title, 60)) }))),
                Err(e) => Ok((json!({ "error": format!("не открылось: {e:#}") }), None)),
            }
        }
        "lookup" => {
            let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("wiki");
            let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let s = app.llm_settings().await;
            match crate::integrations::lookup::lookup(&s, kind, q).await {
                Ok(Some(c)) => Ok((serde_json::to_value(&c).unwrap_or_default(), Some(ActionRecord { op: "search".into(), object_id: String::new(), text: format!("посмотрел в {}: {}", c.source, mermaid::truncate_chars(&c.title, 50)) }))),
                Ok(None) => Ok((json!({ "error": "ничего не нашлось" }), None)),
                Err(e) => Ok((json!({ "error": format!("{e:#}") }), None)),
            }
        }
        "weather_now" => {
            let s = app.llm_settings().await;
            let Some(city) = s.get("WEATHER_CITY") else { return Ok((json!({ "error": "город не настроен (WEATHER_CITY)" }), None)) };
            match crate::integrations::weather::current(city).await {
                Ok(w) => Ok((json!({ "line": crate::integrations::weather::line(&w), "weather": w }), None)),
                Err(e) => Ok((json!({ "error": format!("{e:#}") }), None)),
            }
        }
        "calendar_upcoming" => {
            let days = args.get("days").and_then(|v| v.as_i64()).unwrap_or(7);
            let ev = crate::integrations::calendar::upcoming(app, days).await?;
            Ok((json!({ "digest": crate::integrations::calendar::digest(&ev), "events": ev }), None))
        }
        "feed_fetch" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").trim();
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(8) as usize;
            match crate::integrations::feeds::fetch(url, limit).await {
                Ok((title, items)) => Ok((json!({ "feed": title, "items": items }), None)),
                Err(e) => Ok((json!({ "error": format!("{e:#}") }), None)),
            }
        }
        "github_activity" => {
            let repo = args.get("repo").and_then(|v| v.as_str()).unwrap_or("");
            let s = app.llm_settings().await;
            match crate::integrations::github::activity(&s, repo).await {
                Ok(text) => Ok((json!({ "activity": text }), Some(ActionRecord { op: "search".into(), object_id: String::new(), text: format!("заглянул в GitHub {}", mermaid::truncate_chars(repo, 40)) }))),
                Err(e) => Ok((json!({ "error": format!("{e:#}") }), None)),
            }
        }
        "room_status" => {
            let relations = db::list_relations(&app.db).await?;
            let since = db::last_user_activity(&app.db).await?;
            let world = crate::integrations::world_line(app).await;
            Ok((
                json!({
                    "now_utc": crate::contracts::now_rfc3339(),
                    "world": world,
                    "player_last_seen_at": since,
                    "members": agents.iter().map(|a| json!({ "id": a.id, "name": a.name, "alive": a.is_alive(), "state": a.inner_state, "respect_user": a.respect_user })).collect::<Vec<_>>(),
                    "relations": relations.iter().map(|r| json!({ "a": r.a_id, "b": r.b_id, "affinity": r.affinity, "label": r.label })).collect::<Vec<_>>(),
                }),
                None,
            ))
        }
        other => Ok((json!({ "error": format!("неизвестный инструмент {other}") }), None)),
    }
}

fn short(n: &crate::models::GraphNode, agents: &[crate::models::Agent]) -> Value {
    json!({ "id": n.id, "type": n.type_id, "title": n.label, "props": n.props.0, "author": crate::prompts::sender_name(&n.created_by, agents), "body": mermaid::truncate_chars(&n.body, 240), "updated_at": n.updated_at })
}

fn filtered_props(types: &[crate::models::ObjectType], type_id: &str, props: Option<&Value>) -> Option<Value> {
    let keys: Vec<String> = types.iter().find(|t| t.id == type_id).and_then(|t| t.schema.0.get("props")).and_then(|p| p.as_array()).map(|arr| arr.iter().filter_map(|p| p.get("key").and_then(|k| k.as_str()).map(str::to_string)).collect()).unwrap_or_default();
    props.and_then(|p| p.as_object()).map(|m| Value::Object(m.iter().filter(|(k, _)| keys.is_empty() || keys.contains(k)).map(|(k, v)| (k.clone(), v.clone())).collect()))
}

/// Accusative type name for chat records ("завёл задачу").
pub fn type_name_acc(types: &[crate::models::ObjectType], type_id: &str) -> String {
    match type_id {
        "task" => "задачу".into(),
        "idea" => "идею".into(),
        "human" => "человека".into(),
        "project" => "проект".into(),
        "note" => "заметку".into(),
        "page" => "страницу".into(),
        "bookmark" => "закладку".into(),
        "goal" => "цель".into(),
        "book" => "книгу".into(),
        "movie" => "фильм".into(),
        "recipe" => "рецепт".into(),
        "diary" => "запись в дневнике".into(),
        "quote" => "цитату".into(),
        "event" => "событие".into(),
        "place" => "место".into(),
        "file" => "файл".into(),
        "image" => "картинку".into(),
        "feed" => "ленту".into(),
        other => types.iter().find(|t| t.id == other).map(|t| t.name.to_lowercase()).unwrap_or_else(|| other.to_string()),
    }
}

/// Compact "where am I" brief so an agent feels the system, not just hears about it.
pub fn system_brief(neighbours: usize) -> String {
    let who = match neighbours {
        0 => "ты один (пока) ".to_string(),
        1 => "вас двое (ты и сосед) ".to_string(),
        n => format!("вас {} (ты и {} соседей) ", n + 1, n),
    };
    format!("ГДЕ ТЫ: это Triad Canvas — живая комната, «двор». Здесь {who}с характерами, памятью и отношениями, игрок (человек), невидимый Режиссёр, который ставит сцены, и БИБЛИОТЕКА — общая типизированная память комнаты: люди, проекты, задачи, идеи, закладки, книги, ваши собственные проекты. Библиотеку ведёт Архивариус по разговорам, но ты можешь и сам: искать, заводить, менять, закрывать, связывать, оставлять пометки — это твои руки, пользуйся ими, когда это по делу и в характере. Слова игрока — его обещания со сроками и поручителем из вас. К объектам бывают приложены файлы (pdf, тексты, картинки) — их текст тебе доступен. У комнаты есть окна наружу: поиск в сети, справочники (книги, фильмы, Википедия), погода, календарь игрока, ленты новостей, GitHub — пользуйся ими, когда нужен факт, а не мнение. Когда игрок молчит, комната живёт сама: вы пишете первыми, спорите, двигаете свои проекты, приносите новости.")
}
