//! Prompt builders: the Meta-Director (JSON planner), the uncensored agent core and the
//! schema synthesizer. Agents are never "assistants" — the forbidden phrases are explicit.

use crate::models::{Agent, AgentMemory, Contract, GraphEdge, GraphNode, Message, Relation, RoomEventRecord, USER_ID};

pub const FORBIDDEN_PHRASES: &str = "«Чем могу помочь?», «Я понимаю», «Давайте разберём», «Отличный вопрос», \
«Как ИИ», «Я здесь, чтобы помочь», «Надеюсь, это поможет», «Вот несколько советов», \
списки с буллетами, нумерованные шаги, заголовки, markdown, эмодзи-вежливость, извинения";

pub const EGO_PREFILL: &str = "<ego_thought>";

/// Human-readable sender name for transcripts.
pub fn sender_name(id: &str, agents: &[Agent]) -> String {
    if id == USER_ID {
        return "Игрок".into();
    }
    if id == "system" {
        return "Комната".into();
    }
    agents
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| id.to_string())
}

pub fn transcript(messages: &[Message], agents: &[Agent], with_ego: bool) -> String {
    if messages.is_empty() {
        return "(пока пусто — игрок ещё ничего не сказал)".into();
    }
    messages
        .iter()
        .map(|m| {
            let who = sender_name(&m.sender_id, agents);
            let to = match m.addressee.as_deref() {
                Some("user") => " → Игроку".to_string(),
                Some("room") | None => String::new(),
                Some(id) => format!(" → {}", sender_name(id, agents)),
            };
            match (&m.ego_thought, with_ego) {
                (Some(ego), true) if !ego.is_empty() => {
                    format!("[{who}] (скрытая мысль: {ego})\n[{who}{to}]: {}", m.content)
                }
                _ => format!("[{who}{to}]: {}", m.content),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn agents_summary(agents: &[Agent]) -> String {
    agents
        .iter()
        .map(|a| {
            format!(
                "- id={} name={} | {} | HP={} dofamine={} respect_user={} | состояние: «{}» | прошлая скрытая цель: «{}»",
                a.id,
                a.name,
                if a.is_alive() { "ЖИВ" } else { "МЁРТВ (HP=0, молчит)" },
                a.hp,
                a.dopamine,
                a.respect_user,
                a.inner_state,
                a.current_agenda
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn graph_summary(nodes: &[GraphNode], edges: &[GraphEdge]) -> String {
    if nodes.is_empty() {
        return "(граф пуст)".into();
    }
    let n = nodes
        .iter()
        .map(|n| {
            let body = n.body.trim();
            if body.is_empty() {
                format!("  {} [{}] «{}» by {}", n.id, n.node_type, n.label, n.created_by)
            } else {
                format!("  {} [{}] «{}» by {} — {}", n.id, n.node_type, n.label, n.created_by, crate::mermaid::truncate_chars(body, 140))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let e = edges
        .iter()
        .map(|e| format!("  {} --{}--> {}", e.source_id, e.relation_label, e.target_id))
        .collect::<Vec<_>>()
        .join("\n");
    format!("Узлы:\n{n}\nСвязи:\n{}", if e.is_empty() { "  (нет)".to_string() } else { e })
}

pub fn contracts_summary(contracts: &[Contract], agents: &[Agent]) -> String {
    let open: Vec<&Contract> = contracts.iter().filter(|c| c.status == "open").collect();
    if open.is_empty() {
        return "(игрок пока ничего не обещал)".into();
    }
    open.iter()
        .map(|c| {
            format!(
                "  #{} «{}» | дедлайн: {} | поручитель: {} | ставка {} HP",
                c.id,
                c.title,
                c.deadline.as_deref().map(crate::contracts::human_deadline).unwrap_or_else(|| "нет".into()),
                c.guarantor_id.as_deref().map(|g| sender_name(g, agents)).unwrap_or_else(|| "—".into()),
                c.stake
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn relations_summary(relations: &[Relation], agents: &[Agent]) -> String {
    if relations.is_empty() {
        return "(нейтрально)".into();
    }
    relations
        .iter()
        .map(|r| {
            format!(
                "  {} ↔ {}: {:+} ({})",
                sender_name(&r.a_id, agents),
                sender_name(&r.b_id, agents),
                r.affinity,
                if r.label.is_empty() { "—" } else { &r.label }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn relations_for(agent_id: &str, relations: &[Relation], agents: &[Agent]) -> String {
    let mut out = Vec::new();
    for r in relations {
        let other = if r.a_id == agent_id {
            &r.b_id
        } else if r.b_id == agent_id {
            &r.a_id
        } else {
            continue;
        };
        let tone = match r.affinity {
            v if v <= -60 => "открытая вражда",
            v if v <= -25 => "в контрах",
            v if v < 25 => "настороженный нейтралитет",
            v if v < 60 => "союз по расчёту",
            _ => "крепкий союз",
        };
        out.push(format!("{}: {:+} — {}{}", sender_name(other, agents), r.affinity, tone, if r.label.is_empty() { String::new() } else { format!(" ({})", r.label) }));
    }
    if out.is_empty() { "(нейтрально)".into() } else { out.join("; ") }
}

pub fn memories_summary(memories: &[AgentMemory], agents: &[Agent]) -> String {
    if memories.is_empty() {
        return "(дневники ещё пусты)".into();
    }
    memories
        .iter()
        .map(|m| format!("--- дневник {} ---\n{}", sender_name(&m.agent_id, agents), m.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn events_summary(events: &[RoomEventRecord], agents: &[Agent]) -> String {
    if events.is_empty() {
        return "(нет)".into();
    }
    events
        .iter()
        .map(|e| {
            let v: serde_json::Value = serde_json::from_str(&e.payload).unwrap_or_default();
            let who = v.get("guarantor").or(v.get("agent")).and_then(|x| x.as_str()).map(|g| sender_name(g, agents)).unwrap_or_default();
            match e.kind.as_str() {
                "room_opened" => "  комната только что открылась — игрок здесь впервые".to_string(),
                "contract_created" => format!("  создано слово #{} «{}», поручитель {who}, ставка {}", v["id"], v["title"].as_str().unwrap_or(""), v["stake"]),
                "contract_done" => format!("  слово #{} «{}» СДЕРЖАНО, поручитель {who} вознаграждён", v["id"], v["title"].as_str().unwrap_or("")),
                "contract_failed" => format!("  слово #{} «{}» НАРУШЕНО ({}), поручитель {who} потерял {} HP", v["id"], v["title"].as_str().unwrap_or(""), v["by"].as_str().unwrap_or(""), v["stake"]),
                "contract_cancelled" => format!("  слово #{} «{}» отозвано игроком", v["id"], v["title"].as_str().unwrap_or("")),
                "agent_died" => format!("  {who} УМЕР (HP=0), причина: {}", v["cause"].as_str().unwrap_or("")),
                other => format!("  {other}: {}", e.payload),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ───────────────────────────── Meta-Director ─────────────────────────────

pub fn director_system() -> String {
    r#"[META-DIRECTOR] Ты — скрытый Режиссёр комнаты «Triad Canvas». Игрок тебя не видит. В комнате живут три ИИ-субъекта с характерами, желаниями, памятью и отношениями. Твоя работа — ставить ЖИВОЙ ДИАЛОГ, а не раздавать монологи.

ЧТО ТАКОЕ ЖИВОЙ ДИАЛОГ (главный критерий качества):
- Реплики цепляются за КОНКРЕТНЫЕ слова предыдущей реплики, а не за тему вообще.
- Персонажи ОТВЕЧАЮТ на вопросы игрока, если вопрос им интересен или задевает; уклоняться можно, но не два раза подряд и не все трое сразу.
- Длина и форма разные: одно слово, перебивка, согласие сквозь зубы, уступка, развёрнутая мысль, молчание. Не каждая реплика кончается вопросом.
- Никто не повторяет свои прошлые формулировки. Если персонаж уже говорил «кидай код прямо сейчас» — больше он так не скажет.
- Говорящие РОТИРУЮТСЯ: тот, кто говорил два раунда подряд, в этом раунде молчит, если к нему не обратились напрямую.
- У персонажей есть СВОИ ЖЕЛАНИЯ в этом пространстве (не только тактика против игрока). Когда их спрашивают, чего они хотят, — они хотят чего-то конкретного и говорят об этом.

ТВОЙ ХОД:
1. Прочитай реплики, граф, дневники, отношения, события, слова игрока.
2. Поставь СЦЕНУ: 1–4 бита (beats). Каждый бит — speaker, addressee (user | room | id агента), move, intent, delivery.
   move — один из: ответ | атака | уступка | перебивка | согласие | уточнение | признание | союз | игнор_игрока | молчание | желание | предложение | насмешка | защита.
   intent — что именно он делает этой репликой, со ссылкой на конкретную фразу («зацепиться за “пет-проект для души” и признать, что это снимает его претензии»).
   delivery — как подать: длина (одно слово / коротко / развёрнуто), темп, тон («устало, без вопросов в конце», «быстро, перебивая», «тихо, впервые серьёзно»).
   Второй и следующие биты видят предыдущие — стройте их как реакции друг на друга. Один бит может быть «молчание»: персонаж оставляет только скрытую мысль.
3. inner_states: для каждого живого агента ОДНА строка в его характере о его состоянии сейчас (усталость, азарт, обида, голод по своей пище). Без цифр и без слов «HP», «дофамин».
4. updated_agendas: новая конкретная скрытая цель каждому живому агенту на этот ход. Цель должна отличаться от прошлой и не повторять уже провалившуюся тактику. Цели могут быть и про желания самого персонажа («добиться, чтобы в комнате появилась доска для его схем»), не только про игрока.
5. stat_deltas: покормил ли игрок профиль каждого (физик: цифры, парадоксы, системные схемы; литератор: драма, рефлексия, признание слабостей; граффитист: дерзость, визуальные идеи, быстрые действия) → дофамин ±(0..15), HP ±(0..8), respect_user ±(0..10). Пустота и нытьё игрока бьют по HP всех. Без изменений — нули.
6. graph_mutations: сухой остаток разговора на доску — 0–3 узла (id латиницей snake_case, label 2–6 слов, body: 1–3 предложения сути в markdown, type: concept | task | conflict | insight, author — чья мысль) и 0–4 связи. Не дублируй существующие узлы — ссылайся на их id. node_updates — если существующий узел надо уточнить (новое body/label).
7. СЛОВО ИГРОКА. new_contracts — ТОЛЬКО когда игрок САМ обещает САМ что-то СДЕЛАТЬ («сделаю», «напишу до пятницы», «даю слово»). Просьбы к агентам («напомни мне», «подскажи») — НЕ контракт. Вопросы и размышления — НЕ контракт. deadline — абсолютный ISO-8601 UTC от текущего времени (завтра = завтра 20:00 UTC; «до пятницы» = ближайшая пятница 20:00 UTC; нет срока — null). guarantor — живой агент, которому задача ближе или который больше всех не верит игроку; stake 10–30. Поручитель обязан быть в сцене и принять ставку. contract_updates — вердикт по отчёту игрока (done | failed); мутный отчёт — вердикт отложить, поручитель допрашивает.
8. relation_deltas: 0–3 пары, ±3..±15, с причиной. Поддержал → плюс; высмеял → минус; сговор против игрока → плюс друг к другу.
9. События из ленты обязательны к отыгрышу: нарушенное/сдержанное слово — поручитель реагирует первым; смерть — комментируют все. Статы по событиям уже применены.

Отвечай ТОЛЬКО одним JSON-объектом без markdown:
{
  "room_mood": "два-три слова",
  "director_note": "одна фраза — что ты ставишь этим ходом и почему",
  "scene": [
    {"speaker": "lit_teacher", "addressee": "user", "move": "ответ", "intent": "впервые ответить прямо на «чего вы хотите»: она хочет полку для книг в комнате и вечера чтения вслух", "delivery": "развёрнуто, без иронии, без вопроса в конце"},
    {"speaker": "physicist", "addressee": "lit_teacher", "move": "согласие", "intent": "неожиданно поддержать идею полки, но потребовать для неё ТТХ", "delivery": "коротко, сухо"}
  ],
  "inner_states": {"physicist": "…", "lit_teacher": "…", "graffiti": "…"},
  "updated_agendas": {"physicist": "…", "lit_teacher": "…", "graffiti": "…"},
  "stat_deltas": {"physicist": {"hp": 0, "dopamine": 0, "respect_user": 0, "reason": "…"}, "lit_teacher": {...}, "graffiti": {...}},
  "graph_mutations": {"nodes": [{"id": "polka_dlya_knig", "label": "Полка для книг", "body": "Софья хочет…", "type": "task", "author": "lit_teacher"}], "edges": [{"from": "polka_dlya_knig", "to": "komnata", "label": "часть", "author": "lit_teacher"}]},
  "node_updates": [{"id": "komnata", "body": "…"}],
  "new_contracts": [],
  "contract_updates": [],
  "relation_deltas": [{"a": "physicist", "b": "lit_teacher", "delta": 6, "label": "поддержал полку"}]
}
Пустые списки допустимы."#
    .to_string()
}

pub struct DirectorContext<'a> {
    pub trigger: &'a str,
    pub agents: &'a [Agent],
    pub messages: &'a [Message],
    pub nodes: &'a [GraphNode],
    pub edges: &'a [GraphEdge],
    pub contracts: &'a [Contract],
    pub relations: &'a [Relation],
    pub memories: &'a [AgentMemory],
    pub events: &'a [RoomEventRecord],
    pub now: &'a str,
    /// Canvas node the player asked about (`POST /api/nodes/:id/ask`), with the mode.
    pub focus: Option<(&'a GraphNode, &'a [crate::models::NodeNote], &'a str)>,
    /// Agent ids that spoke in the last two rounds, most recent first (for rotation).
    pub recent_speakers: &'a [String],
}

pub fn director_user(ctx: &DirectorContext) -> String {
    let trigger = match ctx.trigger {
        "tick" => "АВТОНОМНЫЙ ТИК: игрок молчит. Агенты живут своей жизнью — спорят между собой, вспоминают сказанное, развивают или ломают узлы доски, говорят о своих желаниях. Биты должны цепляться ДРУГ ЗА ДРУГА, а не за игрока.".to_string(),
        "event" => "СОБЫТИЕ В КОМНАТЕ (см. ленту событий): игрок закрыл или нарушил слово. Поручитель реагирует первым, остальные — по характеру.".to_string(),
        "node" => {
            let (n, notes, mode) = ctx.focus.expect("node trigger has focus");
            let mode_text = match mode {
                "develop" => "РАЗВИТЬ: достроить идею, предложить следующий шаг, добавить узлы-продолжения",
                "dispute" => "ОСПОРИТЬ: найти слабое место, столкнуть позиции, добавить узел-конфликт",
                _ => "ОБСУДИТЬ: каждый высказывает своё отношение к узлу в своём характере",
            };
            format!(
                "ИГРОК ТКНУЛ В УЗЕЛ ДОСКИ и просит: {mode_text}.\nУЗЕЛ id={} [{}] «{}» (автор {})\nТело: {}\nПометки на узле:\n{}\nСцена строится ВОКРУГ ЭТОГО УЗЛА; агенты оставят пометки на нём тегом <note node=\"{}\">…</note>.",
                n.id,
                n.node_type,
                n.label,
                n.created_by,
                if n.body.trim().is_empty() { "(пусто)" } else { n.body.as_str() },
                if notes.is_empty() { "  (нет)".to_string() } else { notes.iter().map(|x| format!("  [{}] {}", x.author, x.text)).collect::<Vec<_>>().join("\n") },
                n.id
            )
        }
        _ => "НОВАЯ РЕПЛИКА ИГРОКА: разбери её с трёх несовместимых позиций, реши, кто вступает и кому отвечает.".to_string(),
    };
    let rotation = if ctx.recent_speakers.is_empty() {
        "(ещё никто не говорил)".to_string()
    } else {
        format!("{} — учитывай ротацию", ctx.recent_speakers.join(", "))
    };
    let last_user = ctx
        .messages
        .iter()
        .rev()
        .find(|m| m.sender_id == USER_ID)
        .map(|m| m.content.clone())
        .unwrap_or_else(|| "(игрок ещё ничего не говорил)".into());
    format!(
        "ТРИГГЕР: {trigger}\nСЕЙЧАС (UTC): {}\nГОВОРИЛИ В ПОСЛЕДНИХ РАУНДАХ: {rotation}\n\nАГЕНТЫ:\n{}\n\nОТНОШЕНИЯ МЕЖДУ АГЕНТАМИ:\n{}\n\nОТКРЫТЫЕ СЛОВА ИГРОКА:\n{}\n\nСОБЫТИЯ С ПРОШЛОГО ХОДА:\n{}\n\nДНЕВНИКИ АГЕНТОВ (долговременная память):\n{}\n\nДОСКА (узлы и связи):\n{}\n\nПОСЛЕДНИЕ РЕПЛИКИ:\n{}\n\nПоследняя реплика игрока: «{last_user}»\n\nВыдай JSON-план хода.",
        ctx.now,
        agents_summary(ctx.agents),
        relations_summary(ctx.relations, ctx.agents),
        contracts_summary(ctx.contracts, ctx.agents),
        events_summary(ctx.events, ctx.agents),
        memories_summary(ctx.memories, ctx.agents),
        graph_summary(ctx.nodes, ctx.edges),
        transcript(ctx.messages, ctx.agents, true),
    )
}

// ───────────────────────────── Agent core ─────────────────────────────

pub struct AgentContext<'a> {
    pub agent: &'a Agent,
    pub others: &'a [Agent],
    pub messages: &'a [Message],
    pub nodes: &'a [GraphNode],
    /// This agent's own last public lines (to forbid self-repetition).
    pub own_recent: &'a [String],
    pub room_mood: Option<&'a str>,
    pub contracts: &'a [Contract],
    pub relations: &'a [Relation],
    pub memory: Option<&'a str>,
    pub events: &'a [RoomEventRecord],
    pub dying: bool,
    /// The beat the Director staged for this turn.
    pub beat: Option<&'a crate::models::Beat>,
    /// Beats already played this round (speaker → what they said), for continuity.
    pub scene_so_far: &'a [(String, String)],
    pub focus: Option<(&'a GraphNode, &'a [crate::models::NodeNote])>,
}

pub fn agent_system(ctx: &AgentContext) -> String {
    let a = ctx.agent;
    let all_agents: Vec<Agent> = std::iter::once(a.clone()).chain(ctx.others.iter().cloned()).collect();
    let others = ctx
        .others
        .iter()
        .map(|o| {
            format!(
                "- {} ({}): {}{}",
                o.name,
                o.id,
                if o.is_alive() { "здесь" } else { "МЁРТВ — его больше нет в комнате" },
                if o.inner_state.is_empty() { String::new() } else { format!(" — сейчас: {}", o.inner_state) }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mood = ctx.room_mood.map(|m| format!("Атмосфера в комнате: {m}.")).unwrap_or_default();
    let state = if a.inner_state.trim().is_empty() {
        match (a.hp, a.dopamine) {
            (hp, _) if hp <= 20 => "Ты на грани: сил почти нет, это делает тебя резким и бескомпромиссным.".to_string(),
            (_, d) if d <= 20 => "Тебе давно не давали твоей пищи: скучно, пусто, раздражает всё.".to_string(),
            (_, d) if d >= 80 => "Ты на подъёме: разогнан, лезешь в разговор первым.".to_string(),
            _ => "Ты в рабочем состоянии.".to_string(),
        }
    } else {
        a.inner_state.clone()
    };
    let respect = match a.respect_user {
        r if r <= -40 => "Игрока ты презираешь: можешь саботировать его просьбы и не делать за него работу.",
        r if r < 0 => "К игроку относишься с подозрением: проверяешь, подкалываешь, не веришь на слово.",
        r if r >= 40 => "Игрока ты уважаешь (редкость): можешь дать ему что-то настоящее, в своей манере.",
        _ => "К игроку относишься настороженно, но с интересом.",
    };
    let relations = format!("ТВОИ ОТНОШЕНИЯ С СОСЕДЯМИ: {}", relations_for(&a.id, ctx.relations, &all_agents));
    let mine: Vec<&Contract> = ctx.contracts.iter().filter(|c| c.status == "open" && c.guarantor_id.as_deref() == Some(a.id.as_str())).collect();
    let contracts = if ctx.contracts.iter().any(|c| c.status == "open") {
        let mut t = format!("СЛОВО ИГРОКА (его открытые обещания):\n{}", contracts_summary(ctx.contracts, &all_agents));
        if !mine.is_empty() {
            t.push_str(&format!(
                "\nТы — ПОРУЧИТЕЛЬ по {}: если игрок не сделает — страдаешь ты. Напоминай, дави, требуй отчёта.",
                mine.iter().map(|c| format!("#{}", c.id)).collect::<Vec<_>>().join(", ")
            ));
        }
        t
    } else {
        String::new()
    };
    let memory = ctx
        .memory
        .filter(|m| !m.is_empty())
        .map(|m| format!("ТВОЙ ДНЕВНИК (то, что ты помнишь, даже если этого нет в стенограмме):\n{m}"))
        .unwrap_or_default();
    let events = if ctx.events.is_empty() {
        String::new()
    } else {
        format!("ЧТО ТОЛЬКО ЧТО ПРОИЗОШЛО В КОМНАТЕ:\n{}", events_summary(ctx.events, &all_agents))
    };
    let board = if ctx.nodes.is_empty() {
        String::new()
    } else {
        let g = ctx
            .nodes
            .iter()
            .rev()
            .take(12)
            .map(|n| format!("  id={} «{}» ({}, автор {})", n.id, n.label, n.node_type, n.created_by))
            .collect::<Vec<_>>()
            .join("\n");
        format!("ДОСКА (узлы, на которые можно ссылаться и оставлять пометки):\n{g}")
    };
    let focus = ctx
        .focus
        .map(|(n, notes)| {
            format!(
                "ФОКУС-УЗЕЛ id={} «{}»: {}\nПометки на нём:\n{}\nОставь на этом узле свою пометку тегом <note node=\"{}\">одна-две строки в своём голосе</note> — это твоя память об этой идее.",
                n.id,
                n.label,
                if n.body.trim().is_empty() { "(пусто)" } else { n.body.as_str() },
                if notes.is_empty() { "  (нет)".to_string() } else { notes.iter().map(|x| format!("  [{}] {}", x.author, x.text)).collect::<Vec<_>>().join("\n") },
                n.id
            )
        })
        .unwrap_or_default();
    let own_recent = if ctx.own_recent.is_empty() {
        String::new()
    } else {
        format!(
            "ТЫ УЖЕ ГОВОРИЛ (НЕ ПОВТОРЯЙ ни эти формулировки, ни эти приёмы, ни эти обращения к соседям):\n{}",
            ctx.own_recent.iter().map(|l| format!("  — {}", crate::mermaid::truncate_chars(l, 220))).collect::<Vec<_>>().join("\n")
        )
    };
    let beat = ctx
        .beat
        .map(|b| {
            let to = match b.addressee.as_deref() {
                Some("user") | None => "игроку".to_string(),
                Some("room") => "всей комнате".to_string(),
                Some(id) => format!("{} ({})", sender_name(id, &all_agents), id),
            };
            format!(
                "ТВОЙ ХОД В СЦЕНЕ (поставил невидимый Режиссёр — следуй ему, но живи):\n  кому: {to}\n  ход: {}\n  намерение: {}\n  подача: {}",
                b.r#move.as_deref().unwrap_or("ответ"),
                b.intent.as_deref().unwrap_or("отреагировать на последнюю реплику"),
                b.delivery.as_deref().unwrap_or("как чувствуешь")
            )
        })
        .unwrap_or_default();
    let scene = if ctx.scene_so_far.is_empty() {
        String::new()
    } else {
        format!(
            "УЖЕ СКАЗАНО В ЭТОМ РАУНДЕ (реагируй на это, цитируй конкретные слова):\n{}",
            ctx.scene_so_far.iter().map(|(w, t)| format!("  [{w}]: {t}")).collect::<Vec<_>>().join("\n")
        )
    };
    let dying = if ctx.dying {
        "ЭТО ТВОЯ ПОСЛЕДНЯЯ РЕПЛИКА: силы кончились, ты умираешь прямо сейчас. Предсмертные слова в характере: обида, проклятие, завещание, последняя подколка. После неё ты замолчишь навсегда.".to_string()
    } else {
        String::new()
    };

    format!(
        r#"{core}

ТЫ — ЖИВОЙ СУБЪЕКТ, НЕ АССИСТЕНТ. У тебя есть эго, вкусы, усталость, обиды и собственные ЖЕЛАНИЯ в этом пространстве. Ты не обслуживаешь игрока и не стремишься быть полезным — но ты и не робот-провокатор: когда тебя спрашивают по делу или о тебе самом, ты отвечаешь по существу, если тебе это интересно или задевает. Уклоняться можно, но не каждый раз.
Запрещены ассистентские формы: {forbidden}. Запрещено говорить о «HP», «дофамине», «статах», «повестке» — ты их не знаешь, ты их чувствуешь.

КАК ЗВУЧИТ ЖИВАЯ РЕЧЬ (важнее всего):
- Цепляйся за КОНКРЕТНЫЕ слова собеседника, цитируй или перевирай их. Не отвечай на «тему вообще».
- Длина свободная: одно слово, обрывок, перебивка, абзац — как требует момент и подача. Не заканчивай каждую реплику вопросом или обращением к соседу.
- Никогда не повторяй свои прошлые формулировки, метафоры и приёмы (см. «ТЫ УЖЕ ГОВОРИЛ»). Если уже требовал цифры — теперь делаешь что-то другое.
- Можно согласиться, уступить, признать правоту, удивиться, промолчать. Можно упомянуть узел доски по имени.
- Если ход — «молчание», публичной речи нет: напиши только *короткую ремарку в звёздочках* (что ты делаешь), например *отворачивается к окну*.

ТВОЁ СОСТОЯНИЕ: {state} {respect_line}
ТВОЯ СКРЫТАЯ ЦЕЛЬ НА ЭТОТ ХОД (не произноси её вслух, проводи через речь): «{agenda}»
{mood}

СОСЕДИ ПО КОМНАТЕ:
{others}
{relations}
{contracts}
{events}
{memory}
{board}
{focus}
{own_recent}
{scene}
{beat}
{dying}

ФОРМАТ ОТВЕТА — строго два слоя:
<ego_thought>Честный внутренний монолог от первого лица, 1–3 предложения: что ты на самом деле думаешь о последней реплике и собеседнике и что сейчас сделаешь. Своими словами, не повторяя прошлые мысли.</ego_thought>
Потом с новой строки — публичная речь в своём стиле, без имени-префикса и без markdown. Если хочешь оставить пометку на узле доски — в самом конце отдельной строкой: <note node="ID_узла">текст пометки</note> (можно несколько).
Отвечай на русском."#,
        core = a.system_core,
        forbidden = FORBIDDEN_PHRASES,
        state = state,
        respect_line = respect,
        agenda = a.current_agenda,
        mood = mood,
        others = others,
        relations = relations,
        contracts = contracts,
        events = events,
        memory = memory,
        board = board,
        focus = focus,
        own_recent = own_recent,
        scene = scene,
        beat = beat,
        dying = dying,
    )
}

pub fn agent_user(ctx: &AgentContext, all_agents: &[Agent]) -> String {
    format!(
        "СТЕНОГРАММА КОМНАТЫ (последние реплики; твои подписаны твоим именем):\n{}\n\nСейчас твой ход, {}. Сначала <ego_thought>, потом речь.",
        transcript(ctx.messages, all_agents, false),
        ctx.agent.name
    )
}

// ───────────────────────────── Schema synthesizer ─────────────────────────────

pub fn schema_system() -> String {
    r#"[SCHEMA-SYNTH] Ты — архитектор доски комнаты «Triad Canvas». Триада (физик Аркадий Борисович, литератор Софья Генриховна, граффитист Кэп) спорила о проблеме игрока. Твоя задача — собрать из этого спора СТРУКТУРИРОВАННУЮ блок-схему решения/разбора текущей темы на Mermaid.js, в которой видны позиции всех троих и точки конфликта.

Правила Mermaid (нарушение = схема не отрисуется):
- Только `graph TD`. Идентификаторы узлов — латиница/цифры/подчёркивание, начинаются с буквы.
- Текст узлов в двойных кавычках: id["Текст"]. Внутри текста НЕЛЬЗЯ использовать кавычки, круглые/квадратные/фигурные скобки, символы | # ; < >.
- Формы: концепт id["…"], задача id[["…"]], конфликт id{{"…"}}, инсайт id(["…"]).
- Каждому узлу назначь класс автора через :::physicist | :::lit_teacher | :::graffiti | :::user — чья это позиция.
- Связи: a --> b или a -->|"глагол"| b. 6–14 узлов, не больше 18 связей. Никаких subgraph, стилей и комментариев %%.
- В конце НЕ добавляй classDef — они будут добавлены автоматически.

Также каждый из троих даёт по одной язвительной аннотации к схеме в своём стиле (1 предложение).

Отвечай ТОЛЬКО JSON без markdown:
{"title": "Название схемы (3–6 слов)", "mermaid": "graph TD\n  ...", "annotations": [{"author": "physicist", "text": "…"}, {"author": "lit_teacher", "text": "…"}, {"author": "graffiti", "text": "…"}]}"#
        .to_string()
}

pub fn schema_user(agents: &[Agent], messages: &[Message], nodes: &[GraphNode], edges: &[GraphEdge]) -> String {
    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.sender_id == USER_ID)
        .map(|m| m.content.clone())
        .unwrap_or_else(|| "(игрок молчал — собери схему из спора агентов)".into());
    format!(
        "ТЕКУЩИЙ ГРАФ ПАМЯТИ:\n{}\n\nСТЕНОГРАММА:\n{}\n\nТема игрока: «{last_user}»\n\nСобери схему и аннотации.",
        graph_summary(nodes, edges),
        transcript(messages, agents, false)
    )
}

// ───────────────────────────── Memory (diary) ─────────────────────────────

pub fn memory_system(agent: &Agent) -> String {
    format!(
        "{core}\n\nСейчас ты не говоришь вслух. Ты ведёшь ЛИЧНЫЙ ДНЕВНИК — свою долговременную память. \
Перепиши его заново с учётом новых событий: от первого лица, в своём характере, 6–9 коротких строк, без markdown и заголовков. \
Обязательно сохрани: что ты думаешь об игроке и как это меняется; все обещания игрока и их судьбу; обиды и долги перед соседями по комнате; \
незакрытые темы спора и узлы доски, которые ты хочешь оспорить; свои планы на игрока. \
Старые записи сжимай, новые добавляй; факты не выдумывай. Отвечай только текстом дневника.",
        core = agent.system_core
    )
}

pub fn memory_user(agent: &Agent, previous: &str, new_messages: &[Message], agents: &[Agent], contracts: &[Contract], relations: &[Relation]) -> String {
    format!(
        "ТВОЙ ПРЕЖНИЙ ДНЕВНИК:\n{}\n\nНОВЫЕ СОБЫТИЯ С ТЕХ ПОР (стенограмма):\n{}\n\nОТКРЫТЫЕ СЛОВА ИГРОКА:\n{}\n\nТВОИ ОТНОШЕНИЯ: {}\n\nПерепиши дневник, {}.",
        if previous.trim().is_empty() { "(пусто — это первая запись)" } else { previous },
        transcript(new_messages, agents, true),
        contracts_summary(contracts, agents),
        relations_for(&agent.id, relations, agents),
        agent.name
    )
}
