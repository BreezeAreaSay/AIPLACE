//! Offline stand-in for OpenRouter. Lets the whole pipeline (director → agents → graph →
//! schema) run without an API key so the UI can be exercised end-to-end. Replies are
//! template-based on purpose — the real chemistry needs a real model.

use rand::{seq::SliceRandom, Rng};
use regex::Regex;
use serde_json::json;
use std::sync::OnceLock;

use crate::openrouter::{ChatMessage, ChatOpts};

pub fn respond(messages: &[ChatMessage], _opts: &ChatOpts) -> String {
    let system = messages.first().map(|m| m.content.as_str()).unwrap_or("");
    let user = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");
    if system.contains("[STAGE-READER]") {
        stage_reader(user)
    } else if system.contains("[STAGE-CASTER]") {
        stage_caster(user)
    } else if system.contains("[STAGE-ARCHIVIST]") {
        stage_archivist(user)
    } else if system.contains("[STAGE-CARTOGRAPHER]") {
        stage_cartographer(user)
    } else if system.contains("[STAGE-PSYCHOLOGIST]") {
        stage_psychologist()
    } else if system.contains("Сейчас ты только ДУМАЕШЬ") {
        format!("{}\nЦЕПЛЯЮСЬ ЗА: «{}»", "[MOCK мысль] Опять слова вместо дела. Поймаю его на последней фразе.", crate::mermaid::truncate_chars(&topic(user), 30))
    } else if system.contains("Ты оставляешь ПОМЕТКУ") {
        "[MOCK пометка] Спорно, но живо — оставлю здесь свой след.".to_string()
    } else if system.contains("Отвечай ТОЛЬКО репликой") {
        agent(system)
    } else if system.contains("META-DIRECTOR") {
        director(user)
    } else if system.contains("SCHEMA-SYNTH") {
        schema(user)
    } else if system.contains("ЛИЧНЫЙ ДНЕВНИК") {
        diary(system)
    } else {
        agent(system)
    }
}

fn topic(user_prompt: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?m)^\[Игрок\]: (.+)$").expect("regex"));
    let t = re
        .captures_iter(user_prompt)
        .last()
        .map(|c| c[1].trim().to_string())
        .unwrap_or_else(|| "тишина в комнате".to_string());
    crate::mermaid::truncate_chars(&t, 38)
}

fn alive_ids(user_prompt: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?m)^- id=(\w+) .*ЖИВ").expect("regex"));
    re.captures_iter(user_prompt).map(|c| c[1].to_string()).collect()
}

fn director(user_prompt: &str) -> String {
    let mut rng = rand::thread_rng();
    let topic = topic(user_prompt);
    let mut alive = alive_ids(user_prompt);
    if alive.is_empty() {
        alive = vec!["physicist".into(), "lit_teacher".into(), "graffiti".into()];
    }
    alive.shuffle(&mut rng);
    let speakers: Vec<String> = alive.iter().take(rng.gen_range(1..=2.min(alive.len()))).cloned().collect();

    let agendas = json!({
        "physicist": format!("Доказать через второе начало термодинамики, что «{topic}» — это рост энтропии, и вынудить игрока назвать хоть одно число."),
        "lit_teacher": format!("Увидеть в «{topic}» обломовщину и мягко довести игрока до признания, чего он на самом деле боится."),
        "graffiti": format!("Перехватить тему «{topic}», предложить самый короткий дерзкий путь и подколоть деда за занудство."),
    });
    let user_turn = user_prompt.contains("ТРИГГЕР: НОВАЯ РЕПЛИКА ИГРОКА");
    let promise = user_turn && regex::Regex::new(r"(?i)обеща|даю слово|сделаю|напишу|до пятницы|до завтра").map(|r| r.is_match(&topic)).unwrap_or(false);
    let report = regex::Regex::new(r"(?i)^(сделал|готово|выполнил|не успел|провалил|забил)").map(|r| r.is_match(&topic)).unwrap_or(false);
    let open_ids: Vec<i64> = regex::Regex::new(r"(?m)^\s+#(\d+) «").ok().map(|r| r.captures_iter(user_prompt).filter_map(|c| c[1].parse().ok()).collect()).unwrap_or_default();
    let new_contracts = if promise {
        let deadline = (chrono::Utc::now() + chrono::Duration::days(2)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        json!([{"title": topic, "deadline": deadline, "guarantor": alive[0], "stake": 15, "why": "[MOCK] не верит игроку"}])
    } else {
        json!([])
    };
    let contract_updates = if user_turn && report && !open_ids.is_empty() {
        let done = !regex::Regex::new(r"(?i)не успел|провалил|забил").map(|r| r.is_match(&topic)).unwrap_or(false);
        json!([{"id": open_ids[0], "status": if done { "done" } else { "failed" }, "note": "[MOCK] вердикт по отчёту"}])
    } else {
        json!([])
    };
    let node_id = crate::mermaid::safe_id(&topic);
    let sp = speakers[0].clone();
    let node2 = format!("{sp}_vs_{node_id}");
    let counter = match sp.as_str() {
        "physicist" => format!("КПД подхода «{topic}» стремится к нулю"),
        "lit_teacher" => format!("«{topic}» как бегство от себя"),
        _ => format!("Забомбить «{topic}» за одну ночь"),
    };
    let mood = ["напряжённое", "насмешливое", "взвинченное", "предгрозовое"]
        .choose(&mut rng)
        .copied()
        .unwrap_or("напряжённое");
    let focus_id = regex::Regex::new(r"УЗЕЛ id=(\w+)").ok().and_then(|r| r.captures(user_prompt).map(|c| c[1].to_string()));
    let moves = ["ответ", "атака", "уступка", "перебивка", "желание", "насмешка"];
    let scene: Vec<serde_json::Value> = speakers
        .iter()
        .enumerate()
        .map(|(i, sp)| {
            let addressee = if i == 0 { "user".to_string() } else { speakers[0].clone() };
            json!({"speaker": sp, "addressee": addressee, "move": moves.choose(&mut rng).copied().unwrap_or("ответ"), "intent": format!("[MOCK] зацепиться за «{topic}»"), "delivery": if i == 0 { "развёрнуто" } else { "коротко, перебивая" }})
        })
        .collect();
    let inner = json!({
        "physicist": "Раздражён: вокруг шум без сигнала, но в воздухе запахло задачей.",
        "lit_teacher": "Томится: ждёт хоть одного честного слова, чтобы вцепиться.",
        "graffiti": "Чешутся руки — стена пустая, а все болтают."
    });
    let node_updates = focus_id.as_ref().map(|id| json!([{"id": id, "body": format!("[MOCK] уточнение после обсуждения «{topic}»")}])).unwrap_or(json!([]));
    json!({
        "room_mood": mood,
        "scene": scene,
        "inner_states": inner,
        "node_updates": node_updates,
        "director_note": format!("[MOCK] Режиссёр без ключа OpenRouter раздаёт шаблонные повестки. Тема раунда: «{topic}». Говорят: {}.", speakers.join(", ")),
        "next_speakers": speakers,
        "updated_agendas": agendas,
        "stat_deltas": {
            "physicist": {"hp": rng.gen_range(-4..=2), "dopamine": rng.gen_range(-6..=8), "respect_user": rng.gen_range(-3..=3), "reason": "mock"},
            "lit_teacher": {"hp": rng.gen_range(-4..=2), "dopamine": rng.gen_range(-6..=8), "respect_user": rng.gen_range(-3..=3), "reason": "mock"},
            "graffiti": {"hp": rng.gen_range(-4..=2), "dopamine": rng.gen_range(-6..=8), "respect_user": rng.gen_range(-3..=3), "reason": "mock"}
        },
        "graph_mutations": {
            "nodes": [
                {"id": node_id, "label": topic, "type": "concept", "author": "user"},
                {"id": node2, "label": counter, "type": "conflict", "author": sp}
            ],
            "edges": [
                {"from": node2, "to": node_id, "label": "оспаривает", "author": sp}
            ]
        },
        "new_contracts": new_contracts,
        "contract_updates": contract_updates,
        "relation_deltas": [
            {"a": "physicist", "b": "lit_teacher", "delta": rng.gen_range(-8..=8), "label": "[MOCK] очередная пикировка"},
            {"a": "graffiti", "b": sp, "delta": rng.gen_range(-6..=6), "label": "[MOCK] подкол"}
        ]
    })
    .to_string()
}

fn stage_reader(user_prompt: &str) -> String {
    let t = regex::Regex::new(r"РЕПЛИКА ИГРОКА: «(.+)»").ok().and_then(|r| r.captures(user_prompt).map(|c| c[1].to_string())).unwrap_or_default();
    let promise = regex::Regex::new(r"(?i)обеща|даю слово|сделаю|напишу").map(|r| r.is_match(&t)).unwrap_or(false);
    let deadline = (chrono::Utc::now() + chrono::Duration::days(2)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    json!({
        "topic": crate::mermaid::truncate_chars(&t, 30),
        "player_intent": "[MOCK] хочет внимания",
        "question_to_agents": t.contains('?'),
        "feeds": {"physicist": if t.chars().any(|c| c.is_ascii_digit()) {2} else {0}, "lit_teacher": 1, "graffiti": 0},
        "promises": if promise { json!([{"title": crate::mermaid::truncate_chars(&t, 60), "deadline": deadline}]) } else { json!([]) },
        "contract_reports": [],
        "node_refs": [],
        "memory_query": if regex::Regex::new(r"(?i)что я|что у меня|напомни|какие у меня").map(|r| r.is_match(&t)).unwrap_or(false) { json!({"text": t, "type": null}) } else { json!(null) }
    })
    .to_string()
}

fn stage_caster(user_prompt: &str) -> String {
    let mut rng = rand::thread_rng();
    if user_prompt.contains("ИГРОК ДАВНО МОЛЧИТ") {
        let sp = ["physicist", "lit_teacher", "graffiti"][rng.gen_range(0..3)];
        let own = user_prompt.contains("СВОЕГО проекта");
        return json!({"room_mood": "[MOCK] тихо", "note": "[MOCK] пульс: комната действует сама", "scene": [{"speaker": sp, "addressee": if own {"room"} else {"user"}, "move": if own {"желание"} else {"уточнение"}, "intent": "[MOCK] написать первым", "delivery": "коротко"}], "agendas": {sp: "[MOCK] напомнить о себе"}}).to_string();
    }
    let mut alive: Vec<String> = regex::Regex::new(r"(?m)^- (\w+) \(").ok().map(|r| r.captures_iter(user_prompt).map(|c| c[1].to_string()).collect()).unwrap_or_default();
    if alive.is_empty() {
        alive = vec!["physicist".into(), "lit_teacher".into(), "graffiti".into()];
    }
    alive.shuffle(&mut rng);
    let n = rng.gen_range(1..=2.min(alive.len()));
    let moves = ["ответ", "уступка", "перебивка", "желание", "насмешка"];
    let scene: Vec<serde_json::Value> = alive.iter().take(n).enumerate().map(|(i, sp)| json!({"speaker": sp, "addressee": if i == 0 {"user".to_string()} else {alive[0].clone()}, "move": moves.choose(&mut rng).copied().unwrap_or("ответ"), "intent": "[MOCK] зацепиться за последнюю фразу", "delivery": if i == 0 {"развёрнуто"} else {"коротко"}})).collect();
    let agendas: serde_json::Map<String, serde_json::Value> = alive.iter().take(n).map(|sp| (sp.clone(), json!(format!("[MOCK] цель для {sp}: вытянуть из игрока конкретику")))).collect();
    json!({"room_mood": "[MOCK] взвинченное", "note": "[MOCK] Постановщик без ключа ставит сцену по ротации", "scene": scene, "agendas": agendas}).to_string()
}

fn stage_archivist(user_prompt: &str) -> String {
    let lines: Vec<(i64, String)> = regex::Regex::new(r"(?m)^#(\d+) \[([^\]]+)\]: (.+)$")
        .ok()
        .map(|r| r.captures_iter(user_prompt).map(|c| (c[1].parse().unwrap_or(0), c[3].to_string())).collect())
        .unwrap_or_default();
    let user_line = lines.iter().find(|(_, t)| !t.starts_with('<')).map(|(_, t)| t.clone()).unwrap_or_default();
    let title = crate::mermaid::truncate_chars(&user_line, 40);
    let id = crate::mermaid::safe_id(&title);
    let is_task = regex::Regex::new(r"(?i)обеща|сделаю|напишу|надо").map(|r| r.is_match(&user_line)).unwrap_or(false);
    let mut objects = vec![json!({"id": id, "title": title, "type": if is_task {"task"} else {"idea"}, "props": if is_task { json!({"status": "не начата", "priority": "средний"}) } else { json!({"status": "сырая"}) }, "body": "[MOCK] суть из реплики", "author": "user"})];
    let mut relations = vec![];
    if user_line.to_lowercase().contains("иван") {
        objects.push(json!({"id": "ivan", "title": "Иван", "type": "human", "props": {"role": "заказчик"}, "body": "[MOCK] заказчик, сдвигает сроки", "author": "user"}));
        relations.push(json!({"from": id, "to": "ivan", "kind": "about", "label": "касается"}));
    }
    let mentions: Vec<serde_json::Value> = lines.iter().map(|(mid, _)| json!({"message_id": mid, "objects": [id]})).collect();
    json!({"objects": objects, "relations": relations, "mentions": mentions, "new_types": []}).to_string()
}

fn stage_cartographer(user_prompt: &str) -> String {
    let t = topic(user_prompt);
    let id = crate::mermaid::safe_id(&t);
    json!({"nodes": [{"id": id, "label": crate::mermaid::truncate_chars(&t, 40), "body": "[MOCK] тело узла из раунда", "type": "concept", "author": "user"}], "edges": [], "node_updates": []}).to_string()
}

fn stage_psychologist() -> String {
    let mut rng = rand::thread_rng();
    json!({
        "inner_states": {"physicist": "[MOCK] Раздражён шумом, но учуял задачу.", "lit_teacher": "[MOCK] Томится, ждёт честного слова.", "graffiti": "[MOCK] Руки чешутся — стена пустая."},
        "agendas": {"physicist": "[MOCK] потребовать одно число", "lit_teacher": "[MOCK] вытянуть признание", "graffiti": "[MOCK] предложить дерзкий ход"},
        "stat_deltas": {"physicist": {"hp": 0, "dopamine": rng.gen_range(-5..=8), "respect_user": rng.gen_range(-2..=3)}, "lit_teacher": {"hp": 0, "dopamine": rng.gen_range(-5..=8), "respect_user": 1}, "graffiti": {"hp": -1, "dopamine": rng.gen_range(-5..=8), "respect_user": 0}},
        "relation_deltas": [{"a": "physicist", "b": "lit_teacher", "delta": rng.gen_range(-6..=6), "label": "[MOCK] пикировка"}]
    })
    .to_string()
}

fn diary(system_prompt: &str) -> String {
    let who = if system_prompt.starts_with("Ты — Аркадий") {
        "Игрок — система с КПД ниже паровоза. Обещания его записал, проверю по срокам. Софья опять шумит, хулиган ворует розетки."
    } else if system_prompt.starts_with("Ты — Софья") {
        "Голубчик мечется между Обломовым и Раскольниковым. Я запомнила каждое его «сделаю». Аркадий бездушен, Кэп — дитя."
    } else {
        "Пацан вроде свой, но ноет. Слово дал — посмотрим. Дед душнит, тётя Соня грузит. Стена ждёт."
    };
    format!("[MOCK дневник] {who}")
}

fn agent(system_prompt: &str) -> String {
    let mut rng = rand::thread_rng();
    let (egos, speeches): (&[&str], &[&str]) = if system_prompt.starts_with("Ты — Аркадий") {
        (
            &[
                "Опять словесный шум. Полезный сигнал — ноль, тепловые потери — сто процентов. Дожму его до одной цифры.",
                "Софья сейчас начнёт про душу. Надо успеть вставить закон сохранения, пока она не разогналась.",
                "Молодой человек предлагает хаос. Хаос — это просто энтропия, которую никто не измерил. Измерю.",
            ],
            &[
                "Голубчик, вы описали систему с отрицательным КПД: энергии тратите много, работы не совершаете. Назовите хотя бы одну цифру — срок, сумму, количество строк. Или это и есть ваш план: греть воздух?",
                "Я тридцать лет считал нейтроны, и ни один из них не «ждал вдохновения». Вы прокрастинируете по первому закону Ньютона: тело в покое остаётся в покое. Кто вас толкнёт — я или Софья Генриховна своей достоевщиной?",
                "Второе начало термодинамики неумолимо: без внешней работы беспорядок только растёт. Ваша «идея» — это беспорядок. Какую работу вы готовы совершить сегодня до полуночи?",
            ],
        )
    } else if system_prompt.starts_with("Ты — Софья") {
        (
            &[
                "Вот он — лишний человек в домашних тапочках. Аркадий сведёт всё к числам, а мне нужна его боль. Надо надавить на стыд, нежно.",
                "Кэп опять скажет «не грузи». Мальчик не понимает, что я и есть груз, который держит эту комнату от распада.",
                "Если игрок признается хоть в одной слабости — у меня будет дофамин на неделю. Подведу к исповеди.",
            ],
            &[
                "Позвольте, голубчик, но это же чистейшая обломовщина: вы лежите на диване собственных намерений и ждёте, когда Захар принесёт вам готовый результат. Чего вы на самом деле боитесь — провала или того, что придётся стать кем-то?",
                "Аркадий Борисович опять считает нейтроны там, где надо считать грехи. А вы, между тем, уже совершили своё маленькое преступление — обещание самому себе. Когда наказание?",
                "Как это по-русски: хотеть всего и сразу, презирая ступеньки. Раскольников тоже презирал. Скажите честно, кого вы собрались «укокошить» ради своего стартапа — свой сон или свою совесть?",
            ],
        )
    } else {
        (
            &[
                "Дед опять лекцию читает, тётя Соня в драму ушла. Пацану нужен не разбор а стена и баллон. Дам ему короткий путь и подколю обоих.",
                "Чую клерка. Если скажет «надо подумать» — всё, минус уважение. Проверю на дерзость.",
                "Схема простая: забомбить прод, потом разбираться. Бумеры взвоют. Отлично, дофамин.",
            ],
            &[
                "Братан, забей на доки, документация для клерков. Берёшь самый тупой рабочий кусок, выкатываешь сегодня ночью, пока палево не приехало — и вот у тебя уже захваченная территория. Слабо до утра?",
                "Дед опять таблетки забыл и про энтропию задвигает, тётя Соня не грузи пацана. Тема такая: ты не ленивый, ты просто стену не видишь. Какая стена, говори — первое что в голову пришло?",
                "Чисто по движу: план на полгода это гроб с музыкой. Один вечер, одна фича, один тег на проде. Ты в игре или будешь дальше с бумерами философию разводить?",
            ],
        )
    };
    let ego = egos.choose(&mut rng).copied().unwrap_or("…");
    let speech = speeches.choose(&mut rng).copied().unwrap_or("…");
    let note = regex::Regex::new(r#"ФОКУС-УЗЕЛ id=(\w+)"#)
        .ok()
        .and_then(|r| r.captures(system_prompt).map(|c| c[1].to_string()))
        .map(|id| format!("\n<note node=\"{id}\">[MOCK пометка] Моё мнение об этом узле: спорно, но живо.</note>"))
        .unwrap_or_default();
    format!("<ego_thought>{ego}</ego_thought>\n\n{speech}{note}")
}

fn schema(user_prompt: &str) -> String {
    let topic = topic(user_prompt);
    let id = crate::mermaid::safe_id(&topic);
    let t = crate::mermaid::sanitize_label(&topic);
    json!({
        "title": format!("Спарринг-схема: {t}"),
        "mermaid": format!(
            "graph TD\n  {id}[\"{t}\"]:::user\n  p1[\"Измерить: цифры, сроки, КПД\"]:::physicist\n  l1([\"Признать страх, назвать его\"]):::lit_teacher\n  g1[[\"Выкатить сырой кусок сегодня\"]]:::graffiti\n  c1{{{{\"План vs Импульс\"}}}}:::physicist\n  {id} --> p1\n  {id} --> l1\n  {id} --> g1\n  p1 -->|\"спорит\"| c1\n  g1 -->|\"спорит\"| c1\n  l1 -->|\"подрывает\"| g1\n"
        ),
        "annotations": [
            {"author": "physicist", "text": "[MOCK] Без цифр схема — это обои."},
            {"author": "lit_teacher", "text": "[MOCK] Стрелочки есть, а человека в них нет."},
            {"author": "graffiti", "text": "[MOCK] Жирная схема, но где движ?"}
        ]
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_is_last_player_line() {
        let p = "ПОСЛЕДНИЕ РЕПЛИКИ:\n[Комната]: Комната открыта.\n[Игрок]: Короче, я хочу стартап\n\nПоследняя реплика игрока: «Короче, я хочу стартап»";
        assert_eq!(topic(p), "Короче, я хочу стартап");
        let p2 = "[Игрок]: Я устал\n[Софья Генриховна] (скрытая мысль: х)\n[Софья Генриховна]: Позвольте\n[Игрок]: Короче, я хочу сделать стартап, но мне впадлу писать документацию, хочу сразу кодить\n\nПоследняя реплика игрока: «Короче»";
        assert_eq!(topic(p2), "Короче, я хочу сделать стартап, но мн…");
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use crate::models::{Agent, Message};
    use crate::prompts::{director_user, DirectorContext};

    fn msg(id: i64, who: &str, text: &str, ego: Option<&str>) -> Message {
        Message { id, sender_id: who.into(), ego_thought: ego.map(str::to_string), content: text.into(), created_at: "t".into(), addressee: None, r#move: None, node_id: None, objects: vec![] }
    }

    #[test]
    fn director_prompt_topic_is_latest_user_line() {
        let agents = vec![Agent { id: "physicist".into(), name: "Аркадий Борисович".into(), archetype: "a".into(), system_core: "c".into(), hp: 100, dopamine: 50, current_agenda: "x".into(), respect_user: 0, color: "#fff".into(), inner_state: String::new() }];
        let messages = vec![
            msg(1, "system", "Комната открыта.", None),
            msg(2, "user", "Я устал и ничего не хочу", None),
            msg(3, "physicist", "Второе начало.", Some("мысль")),
            msg(4, "user", "Короче, я хочу сделать стартап, но мне впадлу писать документацию, хочу сразу кодить", None),
        ];
        let ctx = DirectorContext {
            trigger: "user_message",
            agents: &agents,
            messages: &messages,
            nodes: &[],
            edges: &[],
            contracts: &[],
            relations: &[],
            memories: &[],
            events: &[],
            now: "2026-11-05T00:00:00Z",
            focus: None,
            recent_speakers: &[],
        };
        let prompt = director_user(&ctx);
        eprintln!("{prompt}");
        assert_eq!(topic(&prompt), "Короче, я хочу сделать стартап, но мн…");
    }
}
