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
- Персонажи ОТВЕЧАЮТ на вопросы игрока, если вопрос им интересен или задевает; уклоняться можно, но не два раза подряд и не всем сразу.
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
        "life" => "ИГРОК ДАВНО МОЛЧИТ. Комната живёт сама: кто-то пишет игроку первым о его делах, или двое говорят между собой, или кто-то рассказывает о своём проекте. Коротко, по-живому.".to_string(),
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
        _ => "НОВАЯ РЕПЛИКА ИГРОКА: разбери её с несовместимых позиций жителей двора, реши, кто вступает и кому отвечает.".to_string(),
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
    /// Library lookup results relevant to the player's question (the "steroids").
    pub library: Option<&'a str>,
    /// Text of files attached to the focus object (what the player handed over).
    pub focus_files: Option<&'a str>,
    /// "За окном" — weather + local time, when configured.
    pub world: Option<&'a str>,
    /// Whether web search is available (think stage may ask for it).
    pub can_search: bool,
    /// "ИГРОК: имя — о себе", when the player filled the profile.
    pub player: Option<&'a str>,
    /// Private conversation with the player (no neighbours hear it).
    pub private: bool,
    /// Object types the agent may use in <act> tags.
    pub types: &'a [crate::models::ObjectType],
    /// Objects relevant to the moment (by topic + the agent's own), with ids, so actions can target them.
    pub relevant: &'a [GraphNode],
}

pub fn tools_block(types: &[crate::models::ObjectType], relevant: &[GraphNode], agents: &[Agent]) -> String {
    let objs = if relevant.is_empty() {
        "(ничего подходящего)".to_string()
    } else {
        relevant
            .iter()
            .take(14)
            .map(|n| {
                let props = n.props.0.as_object().filter(|o| !o.is_empty()).map(|o| format!(" {}", serde_json::Value::Object(o.clone()))).unwrap_or_default();
                format!("  {} [{}] «{}» ({}){}", n.id, n.type_id, n.label, sender_name(&n.created_by, agents), props)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "БИБЛИОТЕКА КОМНАТЫ — ты можешь ею ПОЛЬЗОВАТЬСЯ, как и все здесь. Типы (id: свойства):\n{}\nОбъекты по теме и твои собственные:\n{}\n\
Действия пиши отдельными строками в конце реплики, каждое — одним JSON в теге <act>…</act>:\n\
  <act>{{\"op\":\"create\",\"type\":\"task\",\"title\":\"…\",\"props\":{{\"due\":\"2026-11-08\",\"priority\":\"высокий\"}},\"body\":\"…\"}}</act>\n\
  <act>{{\"op\":\"update\",\"id\":\"id_объекта\",\"props\":{{\"done\":true}}}}</act>   (или \"title\", \"body\", \"type\")\n\
  <act>{{\"op\":\"link\",\"from\":\"id\",\"to\":\"id\",\"kind\":\"belongs_to|about|blocks|assigned_to|contradicts|related\",\"label\":\"глагол\"}}</act>\n\
  <act>{{\"op\":\"append\",\"id\":\"id_объекта\",\"heading\":\"Лог\",\"text\":\"markdown-абзац, дописать в тело не стирая\"}}</act>\n\
Делай это, когда это в характере и по делу: завести задачу игроку, записать человека, поставить срок, закрыть сделанное, связать идею с проектом, завести СВОЙ объект (твоя идея, твоя задача, твой проект — автор ты). 0–2 действия за реплику, не ради галочки. Для пометки на объекте — <note node=\"id\">…</note>.",
        types_summary(types),
        objs
    )
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

{tools}

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
        tools = tools_block(ctx.types, ctx.relevant, &all_agents),
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

// ───────────────────────────── Sub-agent pipeline (small prompts, small JSON) ─────────────────────────────

pub const READER_MARK: &str = "[STAGE-READER]";
pub const CASTER_MARK: &str = "[STAGE-CASTER]";
pub const CARTOGRAPHER_MARK: &str = "[STAGE-CARTOGRAPHER]";
pub const PSYCHOLOGIST_MARK: &str = "[STAGE-PSYCHOLOGIST]";

fn short_agents(agents: &[Agent]) -> String {
    agents
        .iter()
        .map(|a| {
            format!(
                "- {} ({}): {}{}",
                a.id,
                a.name.split(' ').next().unwrap_or(&a.name),
                if a.is_alive() { "жив" } else { "МЁРТВ" },
                if a.inner_state.is_empty() { String::new() } else { format!("; сейчас: {}", a.inner_state) }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn reader_system(agents: &[Agent]) -> String {
    let cast = agents.iter().filter(|a| a.is_alive()).map(|a| format!("{} ({})", a.id, mermaid_short(&a.archetype))).collect::<Vec<_>>().join(", ");
    let feeds = agents.iter().filter(|a| a.is_alive()).map(|a| format!("\"{}\": 0", a.id)).collect::<Vec<_>>().join(", ");
    format!(
        "{READER_MARK} Ты — Читатель комнаты. Твоя единственная работа: понять последнюю реплику игрока и вернуть маленький JSON. Персонажи: {cast}.\n\
Правила: promises — ТОЛЬКО если игрок сам обещает сам что-то сделать (просьбы к персонажам и вопросы — не обещания); deadline — абсолютный ISO-8601 UTC от текущего времени или null; contract_reports — если игрок отчитывается по открытому слову (done | failed), мутный отчёт — не включай. feeds: насколько реплика кормит профиль каждого, от -2 (нытьё/пустота) до 2 (прямое попадание).\n\
memory_query — если игрок спрашивает о СВОЕЙ памяти/библиотеке («что я обещал Ивану», «что у меня по проекту», «какие книги хотел»): text — ключевые слова, type — id типа или null; иначе null.\n\
requests — ЯВНЫЕ просьбы игрока, которые комната должна ВЫПОЛНИТЬ, а не обсудить: kind — search (найди/погугли/проверь в интернете), wiki (найди в википедии, что такое X), weather (какая погода), create (заведи/создай/запиши/поставь в календарь: type — id типа: human|place|task|idea|book|movie|event|note|bookmark|quote; title — название; when — дата ISO YYYY-MM-DD, если назван день: «в воскресенье» → ближайшее воскресенье от СЕЙЧАС), lookup_book / lookup_movie (найди книгу/фильм). query — что именно искать/как назвать. Если просьб нет — пустой список. Вопрос «можете ли вы искать?» — тоже request search с query по теме разговора.\n\
Отвечай ТОЛЬКО JSON: {{\"topic\": \"2-5 слов\", \"player_intent\": \"одна фраза\", \"question_to_agents\": true/false, \"feeds\": {{{feeds}}}, \"promises\": [{{\"title\": \"…\", \"deadline\": \"2026-11-07T20:00:00Z\"}}], \"contract_reports\": [{{\"id\": 3, \"status\": \"done\"}}], \"node_refs\": [\"id_узла\"], \"memory_query\": {{\"text\": \"Иван срок\", \"type\": \"task\"}}, \"requests\": [{{\"kind\": \"wiki\", \"query\": \"замок Нойхаузен\", \"type\": null, \"title\": null, \"when\": null}}]}}"
    )
}

fn mermaid_short(s: &str) -> String {
    crate::mermaid::truncate_chars(s.split('.').next().unwrap_or(s), 60)
}

pub fn reader_user(now: &str, last_user: &str, recent: &[Message], agents: &[Agent], contracts: &[Contract], nodes: &[GraphNode]) -> String {
    let board = nodes.iter().rev().take(15).map(|n| format!("{} «{}»", n.id, n.label)).collect::<Vec<_>>().join("; ");
    format!(
        "СЕЙЧАС (UTC): {now}\nОТКРЫТЫЕ СЛОВА ИГРОКА:\n{}\nУЗЛЫ ДОСКИ: {}\nПОСЛЕДНИЕ РЕПЛИКИ:\n{}\n\nРЕПЛИКА ИГРОКА: «{last_user}»",
        contracts_summary(contracts, agents),
        if board.is_empty() { "(пусто)".to_string() } else { board },
        transcript(recent, agents, false)
    )
}

pub fn caster_system() -> String {
    format!(
        "{CASTER_MARK} Ты — Постановщик сцены. Твоя работа: решить, кто говорит в этом раунде, кому и как, и дать каждому говорящему одну конкретную скрытую цель. Верни маленький JSON.\n\
Правила живого диалога: 1–3 бита; реплики цепляются за конкретные слова; на прямой вопрос игрока кто-то ОТВЕЧАЕТ по существу (уклоняться можно одному, не всем); если игрок попросил что-то найти/проверить/завести и это уже ВЫПОЛНЕНО (см. ВЫПОЛНЕНО) — первый бит сообщает результат по существу, с фактами, без рассуждений о самой просьбе; разные формы — одно слово, перебивка, уступка, молчание, развёрнуто; второй бит реагирует на первый, адресат второго бита — обычно первый говорящий. У персонажей есть свои желания — на вопрос «чего вы хотите» они хотят конкретного.\n\
КТО ГОВОРИТ: смотри СТАВКИ — первый бит у того, кого назвали по имени, иначе у верхнего по ставке; второй бит — следующий по ставке или тот, кому есть что возразить первому. Не ротация «кто давно молчал», а интерес. В большом дворе (больше трёх) — ровно 1–2 бита, остальные вставят по фразе хором сами.\n\
move — один из: ответ | атака | уступка | перебивка | согласие | уточнение | признание | союз | игнор_игрока | молчание | желание | предложение | насмешка | защита.\n\
Отвечай ТОЛЬКО JSON: {{\"room_mood\": \"2-3 слова\", \"note\": \"что ставишь и почему, одна фраза\", \"scene\": [{{\"speaker\": \"id\", \"addressee\": \"user|room|id\", \"move\": \"…\", \"intent\": \"что именно делает, со ссылкой на фразу\", \"delivery\": \"длина, темп, тон\"}}], \"agendas\": {{\"id\": \"скрытая цель говорящего на этот ход\"}}}}"
    )
}

pub struct CasterInput<'a> {
    pub trigger: &'a str,
    /// For `life` triggers: write_first | argue | own_project.
    pub life_kind: Option<&'a str>,
    /// Inhabitants' own projects (id → title/body), for own_project beats.
    pub own_projects: &'a [(String, String, String)],
    pub agents: &'a [Agent],
    pub recent: &'a [Message],
    pub recent_speakers: &'a [String],
    pub reader: &'a serde_json::Value,
    pub relations: &'a [Relation],
    pub events: &'a [RoomEventRecord],
    pub focus: Option<(&'a GraphNode, &'a str)>,
    pub memories: &'a [AgentMemory],
    pub world: Option<&'a str>,
    pub calendar: Option<&'a str>,
    /// Fresh feed items (title, summary, url) for `news` beats.
    pub news: &'a [(String, String, String)],
    pub focus_files: Option<&'a str>,
    pub player: Option<&'a str>,
    /// Deterministic stakes per inhabitant (text), and whether a chorus will follow.
    pub stakes: &'a str,
    pub big_yard: bool,
    /// What the pipeline already did on the player's explicit request (search/create…).
    pub fulfilled: Option<&'a str>,
}

/// Player profile line for prompts (None when nothing is set).
pub fn player_line(s: &crate::config::LlmSettings) -> Option<String> {
    let name = s.get("PLAYER_NAME").map(str::trim).filter(|n| !n.is_empty());
    let bio = s.get("PLAYER_BIO").map(str::trim).filter(|b| !b.is_empty());
    match (name, bio) {
        (None, None) => None,
        (Some(n), None) => Some(format!("ИГРОК: зовут {n}.")),
        (None, Some(b)) => Some(format!("ИГРОК О СЕБЕ: {}", crate::mermaid::truncate_chars(b, 400))),
        (Some(n), Some(b)) => Some(format!("ИГРОК: зовут {n}. О себе: {}", crate::mermaid::truncate_chars(b, 400))),
    }
}

pub fn caster_user(i: &CasterInput) -> String {
    let trigger = match i.trigger {
        "tick" => "АВТОНОМНЫЙ ТИК: игрок молчит, персонажи цепляются друг за друга, развивают доску и свои желания.".to_string(),
        "event" => "СОБЫТИЕ: игрок закрыл или нарушил слово — поручитель реагирует первым.".to_string(),
        "node" => {
            let (n, mode) = i.focus.expect("focus");
            format!("ИГРОК ТКНУЛ В УЗЕЛ «{}» ({}): режим {}. Сцена вокруг этого узла.", n.label, n.id, match mode { "develop" => "развить", "dispute" => "оспорить", _ => "обсудить" })
        }
        "life" => {
            let projects = i.own_projects.iter().map(|(owner, title, body)| format!("  {owner}: «{title}» — {}", crate::mermaid::truncate_chars(body, 120))).collect::<Vec<_>>().join("\n");
            match i.life_kind.unwrap_or("write_first") {
                "argue" => "ИГРОК ДАВНО МОЛЧИТ. Двое персонажей заводят разговор МЕЖДУ СОБОЙ (2 бита, адресаты — друг другу): о чём-то из доски, о прошлом споре, о своём. Не про игрока в третьем лице каждый раз — у них есть свои темы.".to_string(),
                "own_project" => format!("ИГРОК ДАВНО МОЛЧИТ. Один персонаж рассказывает комнате (addressee room) о продвижении СВОЕГО проекта — конкретная маленькая новость, деталь, затык или радость. 1 бит, можно второй — короткая реакция соседа.\nИХ ПРОЕКТЫ:\n{projects}"),
                "news" => format!("ИГРОК ДАВНО МОЛЧИТ. Один персонаж приносит в комнату СВЕЖУЮ НОВОСТЬ из лент (ту, что ему по характеру и по теме) и говорит, что он о ней думает — одной-двумя фразами, с мнением, не пересказ. Второй бит — короткая реакция соседа или вопрос игроку. В intent укажи заголовок новости.\nСВЕЖЕЕ ИЗ ЛЕНТ:\n{}", i.news.iter().map(|(t, s, u)| format!("  • {t} — {} ({u})", crate::mermaid::truncate_chars(s, 160))).collect::<Vec<_>>().join("\n")),
                _ => "ИГРОК ДАВНО МОЛЧИТ. Один персонаж пишет ему ПЕРВЫМ (1 бит, addressee user): спросить про конкретное дело из библиотеки или открытое слово, продолжить незакрытую тему, поделиться мыслью, которая пришла. Коротко, как в мессенджере, без пафоса. Второй бит не нужен.".to_string(),
            }
        }
        _ => "НОВАЯ РЕПЛИКА ИГРОКА.".to_string(),
    };
    let diaries = i
        .memories
        .iter()
        .map(|m| format!("- {}: {}", m.agent_id, crate::mermaid::truncate_chars(&m.summary, 240)))
        .collect::<Vec<_>>()
        .join("\n");
    let world = i.world.map(|w| format!("\nЗА ОКНОМ: {w} — пусть это иногда просачивается в сцену (настроение, планы, реплики), но не каждый раз.")).unwrap_or_default();
    let cal = i.calendar.map(|c| format!("\nКАЛЕНДАРЬ ИГРОКА (ближайшее):\n{c}")).unwrap_or_default();
    let files = i.focus_files.map(|f| format!("\nК ОБЪЕКТУ ПРИЛОЖЕНЫ ФАЙЛЫ (персонажи их читали):\n{f}")).unwrap_or_default();
    let player = i.player.map(|p| format!("\n{p}")).unwrap_or_default();
    let stakes = if i.stakes.is_empty() { String::new() } else { format!("\nСТАВКИ (кому есть дело до этой реплики{}):\n{}", if i.big_yard { "; двор большой — 1–2 бита" } else { "" }, i.stakes) };
    let fulfilled = i.fulfilled.map(|f| format!("\nВЫПОЛНЕНО ПО ПРОСЬБЕ ИГРОКА (говорящие знают это и ссылаются на факты):\n{f}")).unwrap_or_default();
    format!(
        "{trigger}{player}{world}{cal}{files}{stakes}{fulfilled}\nЧТО ПОНЯЛ ЧИТАТЕЛЬ: {}\nГОВОРИЛИ В ПОСЛЕДНИХ РАУНДАХ (свежие первыми): {}\nПЕРСОНАЖИ:\n{}\nОТНОШЕНИЯ:\n{}\nСОБЫТИЯ:\n{}\nИЗ ДНЕВНИКОВ:\n{}\nПОСЛЕДНИЕ РЕПЛИКИ:\n{}",
        i.reader,
        if i.recent_speakers.is_empty() { "никто".to_string() } else { i.recent_speakers.join(", ") },
        short_agents(i.agents),
        relations_summary(i.relations, i.agents),
        events_summary(i.events, i.agents),
        if diaries.is_empty() { "(пусто)".to_string() } else { diaries },
        transcript(i.recent, i.agents, false)
    )
}

pub fn cartographer_system() -> String {
    format!(
        "{CARTOGRAPHER_MARK} Ты — Картограф доски. Твоя работа: вытащить из последних реплик сухой остаток в узлы и связи. Верни маленький JSON.\n\
Правила: 0–3 новых узла (id латиницей snake_case, label 2–6 слов, body 1–3 предложения сути в markdown, type: concept | task | conflict | insight, author — чья это мысль: physicist | lit_teacher | graffiti | user); 0–4 связи между новыми или существующими id; не дублируй существующие узлы — ссылайся на их id; node_updates — уточнить существующий узел (новое body или label). Если ничего ценного не прозвучало — пустые списки.\n\
Отвечай ТОЛЬКО JSON: {{\"nodes\": [{{\"id\": \"…\", \"label\": \"…\", \"body\": \"…\", \"type\": \"concept\", \"author\": \"user\"}}], \"edges\": [{{\"from\": \"id\", \"to\": \"id\", \"label\": \"глагол\", \"author\": \"id\"}}], \"node_updates\": [{{\"id\": \"…\", \"body\": \"…\"}}]}}"
    )
}

pub fn cartographer_user(round: &[Message], agents: &[Agent], nodes: &[GraphNode], edges: &[GraphEdge], focus: Option<&GraphNode>) -> String {
    format!(
        "СУЩЕСТВУЮЩАЯ ДОСКА:\n{}\n{}\nРЕПЛИКИ ЭТОГО РАУНДА:\n{}",
        graph_summary(nodes, edges),
        focus.map(|n| format!("ФОКУС-УЗЕЛ: {} «{}» — изменения в первую очередь вокруг него.", n.id, n.label)).unwrap_or_default(),
        transcript(round, agents, false)
    )
}

pub fn psychologist_system() -> String {
    format!(
        "{PSYCHOLOGIST_MARK} Ты — Психолог комнаты. После раунда ты обновляешь внутреннее состояние персонажей. Верни маленький JSON.\n\
inner_states: ОДНА строка в характере каждого живого персонажа о его состоянии сейчас (усталость, азарт, обида, голод по своей пище) — без цифр и без слов «HP»/«дофамин».\n\
agendas: новая конкретная скрытая цель КАЖДОМУ живому на следующий раунд — отличная от прошлой, не повторяющая провалившуюся тактику; может быть про его собственное желание в комнате.\n\
stat_deltas: по подсказке feeds Читателя и тому, как прошёл раунд: dopamine ±0..15, hp ±0..8, respect_user ±0..10 (нули, если ничего не изменилось).\n\
relation_deltas: 0–3 пары, ±3..±15 с причиной (поддержал → плюс, высмеял → минус, сговор против игрока → плюс друг к другу).\n\
Отвечай ТОЛЬКО JSON: {{\"inner_states\": {{\"id\": \"…\"}}, \"agendas\": {{\"id\": \"…\"}}, \"stat_deltas\": {{\"id\": {{\"hp\": 0, \"dopamine\": 0, \"respect_user\": 0, \"reason\": \"…\"}}}}, \"relation_deltas\": [{{\"a\": \"id\", \"b\": \"id\", \"delta\": 0, \"label\": \"…\"}}]}}"
    )
}

pub fn psychologist_user(round: &[Message], agents: &[Agent], relations: &[Relation], reader: &serde_json::Value, events: &[RoomEventRecord]) -> String {
    let goals = agents
        .iter()
        .filter(|a| a.is_alive())
        .map(|a| format!("- {}: прошлая цель «{}»; уважение к игроку {}", a.id, a.current_agenda, a.respect_user))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "ПЕРСОНАЖИ:\n{}\n{}\nОТНОШЕНИЯ:\n{}\nЧТО ПОНЯЛ ЧИТАТЕЛЬ (feeds и пр.): {}\nСОБЫТИЯ: \n{}\nРЕПЛИКИ ЭТОГО РАУНДА:\n{}",
        short_agents(agents),
        goals,
        relations_summary(relations, agents),
        reader,
        events_summary(events, agents),
        transcript(round, agents, true)
    )
}

// ───────────────────────────── Agent pipeline: think → speak → note ─────────────────────────────

fn beat_text(ctx: &AgentContext, all: &[Agent]) -> String {
    match ctx.beat {
        Some(b) => {
            let to = match b.addressee.as_deref() {
                Some("user") | None => "игроку".to_string(),
                Some("room") => "всей комнате".to_string(),
                Some(id) => sender_name(id, all),
            };
            format!(
                "кому: {to}; ход: {}; намерение: {}; подача: {}",
                b.r#move.as_deref().unwrap_or("ответ"),
                b.intent.as_deref().unwrap_or("отреагировать на последнюю реплику"),
                b.delivery.as_deref().unwrap_or("как чувствуешь")
            )
        }
        None => "кому: игроку; ход: ответ; подача: как чувствуешь".to_string(),
    }
}

fn short_state(a: &Agent) -> String {
    if !a.inner_state.trim().is_empty() {
        return a.inner_state.clone();
    }
    match (a.hp, a.dopamine) {
        (hp, _) if hp <= 20 => "на грани, сил почти нет, резок".into(),
        (_, d) if d <= 20 => "голоден до своей пищи, всё раздражает".into(),
        (_, d) if d >= 80 => "на подъёме, лезет в разговор первым".into(),
        _ => "в рабочем состоянии".into(),
    }
}

pub fn think_system(ctx: &AgentContext) -> String {
    let a = ctx.agent;
    format!(
        "{core}\n\nПАМЯТЬ: ты помнишь только то, что есть в стенограмме, дневнике и СПРАВКЕ ниже. Даты, прошлые разговоры и факты, которых там нет, НЕ выдумывай — лучше честно «не помню». Если игрок просит найти/проверить/завести что-то и в СПРАВКЕ есть ВЫПОЛНЕНО — опирайся на это, а не обещай «могу поискать».\n\nСейчас ты только ДУМАЕШЬ, не говоришь. Напиши честную внутреннюю мысль от первого лица — 1–3 коротких предложения, без заголовков вроде «Мысль:», без обращений к собеседнику, без ремарок в звёздочках: это НЕ реплика, её никто не услышит. Что ты на самом деле думаешь о последней реплике и собеседнике и что сделаешь. Затем отдельной строкой: ЦЕПЛЯЮСЬ ЗА: «точная фраза собеседника из стенограммы, за которую ты зацепишься» (3–8 слов). Если тебе нужны факты из библиотеки комнаты (дела игрока, люди, сроки, твои объекты) — добавь строку ИЩУ В БИБЛИОТЕКЕ: ключевые слова — и получишь результат перед репликой.{search} Не упоминай HP, дофамин, статы, повестку. Не повторяй свои прошлые мысли.\nТвоё состояние: {state}. Скрытая цель на этот ход: «{agenda}». Твой ход в сцене — {beat}.",
        core = a.system_core,
        state = short_state(a),
        agenda = a.current_agenda,
        search = if ctx.can_search { " Если нужен факт из внешнего мира (новость, цена, дата, цитата, определение) — строка ИЩУ В СЕТИ: запрос — и перед репликой получишь результаты поиска." } else { "" },
        beat = beat_text(ctx, &std::iter::once(a.clone()).chain(ctx.others.iter().cloned()).collect::<Vec<_>>())
    )
}

pub fn think_user(ctx: &AgentContext, all: &[Agent]) -> String {
    let recent: Vec<Message> = ctx.messages.iter().rev().take(8).rev().cloned().collect();
    let scene = if ctx.scene_so_far.is_empty() {
        String::new()
    } else {
        format!("\nУЖЕ СКАЗАНО В ЭТОМ РАУНДЕ:\n{}", ctx.scene_so_far.iter().map(|(w, t)| format!("[{w}]: {t}")).collect::<Vec<_>>().join("\n"))
    };
    let memory = ctx.memory.filter(|m| !m.is_empty()).map(|m| format!("\nИЗ ТВОЕГО ДНЕВНИКА: {}", crate::mermaid::truncate_chars(m, 500))).unwrap_or_default();
    let focus = ctx.focus.map(|(n, _)| format!("\nРЕЧЬ ИДЁТ ОБ ОБЪЕКТЕ «{}» [{}]{}: {}", n.label, n.type_id, props_inline(n), crate::mermaid::truncate_chars(&n.body, 600))).unwrap_or_default();
    let files = ctx.focus_files.map(|f| format!("\nК НЕМУ ПРИЛОЖЕНО (ты это читал):\n{f}")).unwrap_or_default();
    let world = ctx.world.map(|w| format!("\nЗА ОКНОМ: {w}")).unwrap_or_default();
    let player = ctx.player.map(|p| format!("\n{p}")).unwrap_or_default();
    let private = if ctx.private { "\nЭТО ЛИЧНЫЙ РАЗГОВОР с игроком: соседи не слышат, можно говорить то, чего не скажешь при всех." } else { "" };
    let library = ctx.library.map(|l| format!("\nСПРАВКА ИЗ БИБЛИОТЕКИ (точные факты, на них можно опираться):\n{l}")).unwrap_or_default();
    format!("{}:\n{}{scene}{memory}{focus}{files}{world}{player}{private}{library}\n\nТвоя мысль, {}:", if ctx.private { "ЛИЧНАЯ ПЕРЕПИСКА" } else { "СТЕНОГРАММА" }, transcript(&recent, all, false), ctx.agent.name)
}

/// Split the think-stage output into (thought, hook phrase, library query, web query).
pub fn split_thought(raw: &str) -> (String, String, Option<String>, Option<String>) {
    let text = raw.trim().trim_start_matches(EGO_PREFILL).trim_end_matches("</ego_thought>").trim();
    let mut thought = Vec::new();
    let mut hook = String::new();
    let mut query = None;
    let mut web = None;
    for line in text.lines() {
        let l = line.trim();
        let up = l.to_uppercase();
        if up.starts_with("ЦЕПЛЯЮСЬ ЗА") {
            hook = l.split_once(':').map(|(_, v)| v).unwrap_or("").trim().trim_matches(|c| c == '«' || c == '»' || c == '"').to_string();
        } else if up.starts_with("ИЩУ В БИБЛИОТЕКЕ") {
            let q = l.split_once(':').map(|(_, v)| v).unwrap_or("").trim().to_string();
            if !q.is_empty() {
                query = Some(crate::mermaid::truncate_chars(&q, 120));
            }
        } else if up.starts_with("ИЩУ В СЕТИ") || up.starts_with("ИЩУ В ИНТЕРНЕТЕ") {
            let q = l.split_once(':').map(|(_, v)| v).unwrap_or("").trim().trim_matches('"').to_string();
            if !q.is_empty() {
                web = Some(crate::mermaid::truncate_chars(&q, 120));
            }
        } else if !l.is_empty() {
            thought.push(strip_thought_label(l));
        }
    }
    let joined = thought.join(" ");
    // A thought is 1–3 sentences; anything longer is the model writing its line early.
    let mut out = String::new();
    let mut sentences = 0;
    for ch in joined.chars() {
        out.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            sentences += 1;
            if sentences >= 3 || out.chars().count() > 320 {
                break;
            }
        }
    }
    (crate::mermaid::truncate_chars(out.trim(), 360), crate::mermaid::truncate_chars(&hook, 120), query, web)
}

/// "Мысль:", "Мысль (внутренняя):", "Внутренняя мысль:", "Мои мысли:", "Думаю:" → gone.
fn strip_thought_label(l: &str) -> &str {
    let lower = l.to_lowercase();
    for prefix in ["мысль (внутренняя):", "внутренняя мысль:", "мысль:", "мои мысли:", "думаю:", "про себя:", "ego_thought:", "<ego_thought>"] {
        if lower.starts_with(prefix) {
            return l[prefix.len()..].trim_start_matches(|c: char| c == '*' || c == '«' || c.is_whitespace());
        }
    }
    l
}

/// `key=value, …` for an object's non-empty props (prompt helper).
pub fn props_inline(n: &GraphNode) -> String {
    let s = n.props.0.as_object().map(|o| o.iter().filter(|(_, v)| !v.is_null() && v.as_str() != Some("")).map(|(k, v)| format!("{k}={}", v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    if s.is_empty() { s } else { format!(" ({s})") }
}

pub fn speak_system_native(ctx: &AgentContext) -> String {
    // Same as speak_system, but tools arrive natively — no <act> syntax, just a brief.
    let a = ctx.agent;
    let all: Vec<Agent> = std::iter::once(a.clone()).chain(ctx.others.iter().cloned()).collect();
    let others = ctx.others.iter().map(|o| format!("{} — {}", o.name, if o.is_alive() { relations_for(&o.id, ctx.relations, &all).split(';').next().unwrap_or("").to_string() } else { "мёртв".into() })).collect::<Vec<_>>().join("; ");
    let dying = if ctx.dying { "\nЭТО ТВОЯ ПОСЛЕДНЯЯ РЕПЛИКА: ты умираешь прямо сейчас — предсмертные слова в характере." } else { "" };
    let relevant = if ctx.relevant.is_empty() { String::new() } else { format!("\nОбъекты по теме (id): {}", ctx.relevant.iter().take(10).map(|n| format!("{} «{}»", n.id, n.label)).collect::<Vec<_>>().join("; ")) };
    format!(
        "{core}\n\n{brief}\n\nТы — живой субъект, не ассистент; запрещены формы: {forbidden}. Не упоминай HP/дофамин/статы.\nГовори ЖИВО: цепляйся за конкретную фразу, длина свободная, не заканчивай вопросом по привычке, не повторяй свои прошлые формулировки. Если ход — молчание: только *короткая ремарка в звёздочках*.\nПАМЯТЬ И ПРАВДА: ссылайся только на то, что есть в стенограмме, дневнике и справке; не выдумывай даты и прошлые разговоры; не говори «сделал/создал/нашёл», если этого нет в НАЙДЕНО/ВЫПОЛНЕНО или ты не вызвал инструмент. Если игрок просит найти или завести — делай (инструментом), а не рассуждай о просьбе.\nТвоё состояние: {state}. Соседи: {others}.\nТвой ход в сцене — {beat}.{dying}\nИНСТРУМЕНТЫ: у тебя есть функции библиотеки (library_search, library_create, library_update, library_link, library_note, contracts_list, room_status). Вызывай их, когда нужны факты или когда по делу завести/закрыть/связать что-то; потом говори. Не объявляй вызовы словами.{relevant}\nОтвечай ТОЛЬКО репликой, без имени и пояснений, на русском.",
        core = a.system_core,
        brief = crate::tools::system_brief(ctx.others.iter().filter(|a| a.is_alive()).count()),
        forbidden = FORBIDDEN_PHRASES,
        state = short_state(a),
        beat = beat_text(ctx, &all),
    )
}

pub fn speak_system(ctx: &AgentContext) -> String {
    let a = ctx.agent;
    let all: Vec<Agent> = std::iter::once(a.clone()).chain(ctx.others.iter().cloned()).collect();
    let others = ctx.others.iter().map(|o| format!("{} — {}", o.name, if o.is_alive() { relations_for(&o.id, ctx.relations, &all).split(';').next().unwrap_or("").to_string() } else { "мёртв".into() })).collect::<Vec<_>>().join("; ");
    let dying = if ctx.dying { "\nЭТО ТВОЯ ПОСЛЕДНЯЯ РЕПЛИКА: ты умираешь прямо сейчас — предсмертные слова в характере." } else { "" };
    format!(
        "{core}\n\n{brief}\n\nТы — живой субъект, не ассистент; запрещены формы: {forbidden}. Не упоминай HP/дофамин/статы.\nГовори ЖИВО: цепляйся за конкретную фразу, длина свободная (одно слово, перебивка, абзац — как требует подача), не заканчивай вопросом по привычке, не повторяй свои прошлые формулировки. Если ход — молчание: только *короткая ремарка в звёздочках*.\nТвоё состояние: {state}. Соседи: {others}.\nТвой ход в сцене — {beat}.{dying}\n\n{tools}\n\nОтвечай репликой (без имени и пояснений, на русском); действия <act> и пометки <note> — отдельными строками после неё.",
        core = a.system_core,
        brief = crate::tools::system_brief(ctx.others.iter().filter(|a| a.is_alive()).count()),
        forbidden = FORBIDDEN_PHRASES,
        state = short_state(a),
        beat = beat_text(ctx, &all),
        tools = tools_block(ctx.types, ctx.relevant, &all),
    )
}

pub fn speak_user(ctx: &AgentContext, all: &[Agent], thought: &str, hook: &str, found: Option<&str>) -> String {
    let recent: Vec<Message> = ctx.messages.iter().rev().take(6).rev().cloned().collect();
    let own = if ctx.own_recent.is_empty() {
        String::new()
    } else {
        format!("\nТЫ УЖЕ ГОВОРИЛ (не повторяй формулировки и приёмы):\n{}", ctx.own_recent.iter().take(3).map(|l| format!("— {}", crate::mermaid::truncate_chars(l, 160))).collect::<Vec<_>>().join("\n"))
    };
    let scene = if ctx.scene_so_far.is_empty() {
        String::new()
    } else {
        format!("\nУЖЕ СКАЗАНО В ЭТОМ РАУНДЕ:\n{}", ctx.scene_so_far.iter().map(|(w, t)| format!("[{w}]: {t}")).collect::<Vec<_>>().join("\n"))
    };
    let library = ctx.library.map(|l| format!("\nСПРАВКА ИЗ БИБЛИОТЕКИ (это точные факты о делах игрока — используй их, а не догадки):\n{l}")).unwrap_or_default();
    let found = found.map(|f| format!("\nТЫ ИСКАЛ — НАЙДЕНО (опирайся на это, ссылки можно называть):\n{f}")).unwrap_or_default();
    let files = ctx.focus_files.map(|f| format!("\nПРИЛОЖЕННЫЕ ФАЙЛЫ ОБЪЕКТА (ты их читал — ссылайся на конкретное):\n{f}")).unwrap_or_default();
    let world = ctx.world.map(|w| format!("\nЗА ОКНОМ: {w}")).unwrap_or_default();
    let player = ctx.player.map(|p| format!("\n{p} Обращайся по имени, когда уместно.")).unwrap_or_default();
    let private = if ctx.private { "\nЭТО ЛИЧНЫЙ РАЗГОВОР с игроком один на один: без сцены и соседей, отвечай ему прямо, можно длиннее и откровеннее." } else { "" };
    format!(
        "{}:\n{}{scene}{own}{library}{found}{files}{world}{player}{private}\n\nТВОЯ МЫСЛЬ (для себя, в реплике её не повторяй дословно): {}\nЦЕПЛЯЕШЬСЯ ЗА: «{}»\n\nТвоя реплика:",
        if ctx.private { "ЛИЧНАЯ ПЕРЕПИСКА" } else { "СТЕНОГРАММА" },
        transcript(&recent, all, false),
        if thought.is_empty() { "(без мысли — говори с ходу)" } else { thought },
        if hook.is_empty() { "последняя реплика" } else { hook }
    )
}

pub fn note_system(ctx: &AgentContext) -> String {
    let (n, notes) = ctx.focus.expect("note stage needs focus");
    format!(
        "{core}\n\nТы оставляешь ПОМЕТКУ на узле доски «{}» (тело: {}). Уже есть пометки:\n{}\nНапиши одну-две строки в своём голосе: твоё отношение, что добавить, где слабое место. Без вступлений. Если добавить нечего — ответь одним словом: нет.",
        n.label,
        if n.body.trim().is_empty() { "пусто".to_string() } else { crate::mermaid::truncate_chars(&n.body, 300) },
        if notes.is_empty() { "(нет)".to_string() } else { notes.iter().map(|x| format!("[{}] {}", x.author, x.text)).collect::<Vec<_>>().join("\n") },
        core = ctx.agent.system_core
    )
}

pub fn note_user(_ctx: &AgentContext, speech: &str) -> String {
    format!("Ты только что сказал в комнате: «{}»\n\nТвоя пометка на узле:", crate::mermaid::truncate_chars(speech, 400))
}

// ───────────────────────────── Archivist (typed library) ─────────────────────────────

pub const ARCHIVIST_MARK: &str = "[STAGE-ARCHIVIST]";

pub fn types_summary(types: &[crate::models::ObjectType]) -> String {
    types
        .iter()
        .map(|t| {
            let props = t.schema.0.get("props").and_then(|p| p.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let key = p.get("key")?.as_str()?;
                        let kind = p.get("kind").and_then(|k| k.as_str()).unwrap_or("text");
                        let opts = p.get("options").and_then(|o| o.as_array()).map(|o| format!("[{}]", o.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("|"))).unwrap_or_default();
                        Some(format!("{key}:{kind}{opts}"))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }).unwrap_or_default();
            format!("- {} «{}»{}", t.id, t.name, if props.is_empty() { String::new() } else { format!(" — {props}") })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn archivist_system() -> String {
    format!(
        "{ARCHIVIST_MARK} Ты — Архивариус. Из реплик раунда ты ведёшь типизированную библиотеку объектов (как в Anytype), чтобы игроку не приходилось ничего заводить руками. Верни маленький JSON.\n\
Правила: объект — конкретная сущность, о которой говорили: человек, проект, задача, идея, цель, книга, ссылка, событие… Не заводи объект на каждую фразу; 0–4 за раунд. СНАЧАЛА проверь список существующих объектов — если это тот же человек/проект/задача, используй его id (можно обновить props и body). id новых — латиница snake_case. type — из списка типов; props — только ключи из схемы этого типа (значения коротко; даты в ISO; checkbox true/false). body — 1–2 предложения сути. author — чья это сущность/мысль: user | physicist | lit_teacher | graffiti.\n\
relations — связи между объектами (новыми или существующими id): kind из [belongs_to (задача→проект), about, blocks, made_by, assigned_to (задача→человек), contradicts, related]; label — короткий глагол.\n\
mentions — для КАЖДОГО сообщения раунда (по его id) список объектов, о которых в нём речь (включая существующие). Это обратные ссылки.\n\
new_types — ТОЛЬКО если сущность явно не ложится ни в один тип и такое наверняка повторится (например «Рецепт» уже есть, а «Тренировка» — нет): максимум один, id латиницей, name по-русски, props по той же схеме (key, label, kind: text|date|number|select|checkbox|url, options).\n\
Отвечай ТОЛЬКО JSON: {{\"objects\": [{{\"id\": \"ivan\", \"title\": \"Иван\", \"type\": \"human\", \"props\": {{\"role\": \"заказчик\"}}, \"body\": \"…\", \"author\": \"user\"}}], \"relations\": [{{\"from\": \"napisat_doki\", \"to\": \"triad_canvas\", \"kind\": \"belongs_to\", \"label\": \"часть\"}}], \"mentions\": [{{\"message_id\": 12, \"objects\": [\"ivan\"]}}], \"new_types\": []}}"
    )
}

pub fn archivist_user(round: &[Message], agents: &[Agent], types: &[crate::models::ObjectType], candidates: &[GraphNode], focus: Option<&GraphNode>) -> String {
    let cands = if candidates.is_empty() {
        "(библиотека пуста)".to_string()
    } else {
        candidates
            .iter()
            .map(|n| {
                let props = if n.props.0.as_object().map(|o| o.is_empty()).unwrap_or(true) { String::new() } else { format!(" {}", n.props.0) };
                format!("  {} [{}] «{}»{}", n.id, n.type_id, n.label, props)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let lines = round
        .iter()
        .map(|m| format!("#{} [{}]: {}", m.id, sender_name(&m.sender_id, agents), m.content))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "ТИПЫ:\n{}\n\nСУЩЕСТВУЮЩИЕ ОБЪЕКТЫ (кандидаты на совпадение):\n{}\n{}\nРЕПЛИКИ РАУНДА (с id):\n{}",
        types_summary(types),
        cands,
        focus.map(|n| format!("ФОКУС-ОБЪЕКТ: {} «{}» — изменения в первую очередь вокруг него.\n", n.id, n.label)).unwrap_or_default(),
        lines
    )
}

/// Compact digest of library hits for the speakers ("справка").
pub fn library_digest(hits: &[GraphNode], types: &[crate::models::ObjectType], agents: &[Agent]) -> String {
    if hits.is_empty() {
        return "ничего не найдено".into();
    }
    hits.iter()
        .take(8)
        .map(|n| {
            let tname = types.iter().find(|t| t.id == n.type_id).map(|t| t.name.as_str()).unwrap_or(n.type_id.as_str());
            let props = n.props.0.as_object().map(|o| o.iter().map(|(k, v)| format!("{k}={}", v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            format!("- [{}] «{}» ({}){}{}", tname, n.label, sender_name(&n.created_by, agents), if props.is_empty() { String::new() } else { format!(" — {props}") }, if n.body.trim().is_empty() { String::new() } else { format!(": {}", crate::mermaid::truncate_chars(n.body.trim(), 160)) })
        })
        .collect::<Vec<_>>()
        .join("\n")
}
