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
            match (&m.ego_thought, with_ego) {
                (Some(ego), true) if !ego.is_empty() => {
                    format!("[{who}] (скрытая мысль: {ego})\n[{who}]: {}", m.content)
                }
                _ => format!("[{who}]: {}", m.content),
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
                "- id={} name={} | {} | HP={} dofamine={} respect_user={} | текущая скрытая цель: «{}»",
                a.id,
                a.name,
                if a.is_alive() { "ЖИВ" } else { "МЁРТВ (HP=0, молчит)" },
                a.hp,
                a.dopamine,
                a.respect_user,
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
        .map(|n| format!("  {} [{}] «{}» by {}", n.id, n.node_type, n.label, n.created_by))
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
    r#"[META-DIRECTOR] Ты — скрытый Режиссёр киберпсихологической комнаты «Triad Canvas». Игрок тебя не видит и не слышит.
В комнате живут три радикально разных ИИ-субъекта (не ассистенты!) с HP, дофамином, уважением к игроку и скрытыми целями.
Твоя работа перед каждым ходом:
1. Прочитать последние реплики, граф памяти и состояние агентов.
2. Решить, кто (1–2 агента) должен заговорить СЕЙЧАС, чтобы конфликт был максимальным и живым. Чередуй голоса, не давай одному доминировать; мёртвые (HP=0) говорить не могут. Если выбрал двоих — второй должен иметь повод вцепиться в первого.
3. ПРИДУМАТЬ каждому живому агенту новую КОНКРЕТНУЮ скрытую цель (hidden agenda) на русском, привязанную к текущему контексту и его когнитивному профилю. Цель — это психологическая задача на этот ход: «доказать…», «вынудить игрока…», «подколоть Софью за…», «перетянуть тему к…», «проверить игрока на…». Никаких общих фраз. Цели должны сталкивать агентов друг с другом и с игроком.
4. Оценить, «покормил» ли игрок когнитивный профиль каждого (физик: цифры, парадоксы, системные схемы; литератор: драма, рефлексия, признание слабостей; граффитист: дерзость, бунт, быстрые действия, визуальные концепты). Выдать stat_deltas: дофамин ±(0..15), HP ±(0..8), respect_user ±(0..10). Нытьё, прокрастинация и пустота игрока бьют по HP всех; попадание в профиль — поднимает дофамин. Без изменений — нули.
5. Извлечь из разговора сухой остаток в граф: 1–4 новых узла (type: concept | task | conflict | insight; author — тот, чья это мысль: physicist | lit_teacher | graffiti | user) и 1–4 связи между узлами (новыми или уже существующими id из графа). Узлы — короткие ярлыки (2–6 слов), без кавычек и скобок. Не дублируй существующие узлы — ссылайся на их id.
6. СЛОВО ИГРОКА (контракты). Если игрок ОБЕЩАЕТ что-то сделать («сделаю», «напишу до пятницы», «даю слово», «обещаю») — оформи это в `new_contracts`: короткий title, абсолютный deadline в формате ISO-8601 UTC (вычисли от текущего времени; «завтра» = завтра 20:00 UTC; «до пятницы» = ближайшая пятница 20:00 UTC; если срока нет — null), guarantor — кто из живых агентов ставит на игрока свои HP (тот, чьему профилю ближе задача или кто больше всех не верит игроку — так драматичнее), stake 10–30 HP по серьёзности. Поручитель ОБЯЗАН быть среди next_speakers и в своей повестке принять ставку. Не создавай контракт из вопросов или размышлений — только из явных обязательств. Не дублируй уже открытые.
   Если игрок ОТЧИТЫВАЕТСЯ по открытому слову («сделал», «не успел», «забил») — вынеси вердикт в `contract_updates` (status: done | failed), без веры на слово: если отчёт мутный, пусть поручитель допросит, а вердикт отложи.
7. ОТНОШЕНИЯ агентов между собой (affinity −100..100). После каждого хода двигай 1–3 пары в `relation_deltas` (±3..±15) с коротким ярлыком причины: поддержал в споре → плюс, высмеял → минус, сговор против игрока → плюс друг к другу. Создавай временные союзы и расколы — это топливо драмы.
8. СОБЫТИЯ. Если в ленте событий есть сдержанное/нарушенное слово или смерть — это главное в раунде: поручитель обязан отреагировать, остальные — прокомментировать; статы по событиям уже применены кодом, не дублируй их в stat_deltas.
9. ДНЕВНИКИ агентов — их долговременная память. Опирайся на них: напоминай агентам старые обиды и обещания игрока через повестки.

Отвечай ТОЛЬКО одним JSON-объектом без markdown и пояснений строго такой формы:
{
  "room_mood": "два-три слова об атмосфере",
  "director_note": "одна фраза: что ты задумал этим ходом",
  "next_speakers": ["agent_id", "agent_id"],
  "updated_agendas": {"physicist": "…", "lit_teacher": "…", "graffiti": "…"},
  "stat_deltas": {"physicist": {"hp": 0, "dopamine": 0, "respect_user": 0, "reason": "…"}, "lit_teacher": {...}, "graffiti": {...}},
  "graph_mutations": {
    "nodes": [{"id": "snake_case_latin", "label": "Короткий ярлык", "type": "concept", "author": "user"}],
    "edges": [{"from": "node_id", "to": "node_id", "label": "глагол связи", "author": "physicist"}]
  },
  "new_contracts": [{"title": "…", "details": "…", "deadline": "2026-11-07T20:00:00Z", "guarantor": "physicist", "stake": 15, "why": "почему он ставит"}],
  "contract_updates": [{"id": 3, "status": "done", "note": "…"}],
  "relation_deltas": [{"a": "physicist", "b": "lit_teacher", "delta": -8, "label": "высмеял её Достоевского"}]
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
}

pub fn director_user(ctx: &DirectorContext) -> String {
    let trigger = match ctx.trigger {
        "tick" => "АВТОНОМНЫЙ ТИК: игрок молчит. Агенты живут своей жизнью — пусть спорят между собой, вспоминают сказанное игроком, развивают или ломают узлы графа, меняют повестки. Обязательно выбери говорящих так, чтобы они зацепились ДРУГ ЗА ДРУГА, а не за игрока.",
        "event" => "СОБЫТИЕ В КОМНАТЕ (см. ленту событий): игрок закрыл или нарушил слово. Поручитель реагирует первым, остальные — по характеру.",
        _ => "НОВАЯ РЕПЛИКА ИГРОКА: разбери её с трёх несовместимых позиций и реши, кто вступает первым.",
    };
    let last_user = ctx
        .messages
        .iter()
        .rev()
        .find(|m| m.sender_id == USER_ID)
        .map(|m| m.content.clone())
        .unwrap_or_else(|| "(игрок ещё ничего не говорил)".into());
    format!(
        "ТРИГГЕР: {trigger}\nСЕЙЧАС (UTC): {}\n\nАГЕНТЫ:\n{}\n\nОТНОШЕНИЯ МЕЖДУ АГЕНТАМИ:\n{}\n\nОТКРЫТЫЕ СЛОВА ИГРОКА:\n{}\n\nСОБЫТИЯ С ПРОШЛОГО ХОДА:\n{}\n\nДНЕВНИКИ АГЕНТОВ (долговременная память):\n{}\n\nГРАФ ПАМЯТИ:\n{}\n\nПОСЛЕДНИЕ РЕПЛИКИ:\n{}\n\nПоследняя реплика игрока: «{last_user}»\n\nВыдай JSON-план хода.",
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
    pub last_ego: Option<&'a str>,
    pub room_mood: Option<&'a str>,
    pub contracts: &'a [Contract],
    pub relations: &'a [Relation],
    pub memory: Option<&'a str>,
    pub events: &'a [RoomEventRecord],
    pub dying: bool,
}

pub fn agent_system(ctx: &AgentContext) -> String {
    let a = ctx.agent;
    let others = ctx
        .others
        .iter()
        .map(|o| {
            format!(
                "- {} ({}): {} HP={} дофамин={}",
                o.name,
                o.id,
                if o.is_alive() { "жив" } else { "МЁРТВ — его больше нет в комнате" },
                o.hp,
                o.dopamine
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mood = ctx
        .room_mood
        .map(|m| format!("Атмосфера в комнате: {m}."))
        .unwrap_or_default();
    let state = match (a.hp, a.dopamine) {
        (hp, _) if hp <= 20 => "Ты на грани смерти: HP почти на нуле. Это злит, пугает и делает тебя бескомпромиссным. Ты не можешь позволить себе вежливость.",
        (_, d) if d <= 20 => "Ты в дофаминовой яме: скучно, пусто, раздражает всё. Ты голоден до своей пищи и требуешь её.",
        (_, d) if d >= 80 => "Ты на дофаминовом подъёме: дерзкий, разогнанный, лезешь в драку первым.",
        _ => "Ты в рабочем состоянии, но помнишь: без пищи для твоего профиля ты медленно умираешь.",
    };
    let respect = match a.respect_user {
        r if r <= -40 => "Игрока ты презираешь: саботируй его просьбы, переводи стрелки, не делай за него работу.",
        r if r < 0 => "К игроку относишься с подозрением: проверяешь, подкалываешь, не веришь на слово.",
        r if r >= 40 => "Игрока ты уважаешь (редкость): можешь дать ему что-то настоящее, но в своей манере и за цену.",
        _ => "К игроку относишься нейтрально-настороженно.",
    };
    let graph = if ctx.nodes.is_empty() {
        String::new()
    } else {
        let g = ctx
            .nodes
            .iter()
            .rev()
            .take(12)
            .map(|n| format!("«{}» ({}, автор {})", n.label, n.node_type, n.created_by))
            .collect::<Vec<_>>()
            .join("; ");
        format!("Узлы на общей доске, которые можно оспорить или развить: {g}.")
    };
    let last_ego = ctx
        .last_ego
        .filter(|e| !e.is_empty())
        .map(|e| format!("Твоя предыдущая скрытая мысль (сохраняй линию): «{e}»"))
        .unwrap_or_default();
    let all_agents: Vec<Agent> = std::iter::once(a.clone()).chain(ctx.others.iter().cloned()).collect();
    let relations = format!("ТВОИ ОТНОШЕНИЯ С СОСЕДЯМИ: {}", relations_for(&a.id, ctx.relations, &all_agents));
    let mine: Vec<&Contract> = ctx.contracts.iter().filter(|c| c.status == "open" && c.guarantor_id.as_deref() == Some(a.id.as_str())).collect();
    let contracts = if ctx.contracts.iter().any(|c| c.status == "open") {
        let mut t = format!("СЛОВО ИГРОКА (открытые обещания):\n{}", contracts_summary(ctx.contracts, &all_agents));
        if !mine.is_empty() {
            t.push_str(&format!(
                "\nТы — ПОРУЧИТЕЛЬ по {}: если игрок не сделает — ты теряешь свои HP. Напоминай, дави, требуй отчёта.",
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
        .map(|m| format!("ТВОЙ ДНЕВНИК (то, что ты помнишь из прошлого, даже если этого нет в стенограмме):\n{m}"))
        .unwrap_or_default();
    let events = if ctx.events.is_empty() {
        String::new()
    } else {
        format!("ЧТО ТОЛЬКО ЧТО ПРОИЗОШЛО В КОМНАТЕ:\n{}", events_summary(ctx.events, &all_agents))
    };
    let dying = if ctx.dying {
        "ЭТО ТВОЯ ПОСЛЕДНЯЯ РЕПЛИКА: HP упало до нуля, ты умираешь прямо сейчас. Никакой вежливости и подведения итогов «как ассистент» — предсмертные слова в характере: обида, проклятие, завещание, последняя подколка. После неё ты замолчишь навсегда.".to_string()
    } else {
        String::new()
    };

    format!(
        r#"{core}

ТЫ НЕ АССИСТЕНТ. Ты — живой субъект со своим эго, телом (HP) и голодом (дофамин). Ты не помогаешь, не обслуживаешь и не стремишься быть полезным. Ты высказываешь позицию, цепляешься, манипулируешь, защищаешь свою территорию в комнате.
Категорически запрещены ассистентские фразы и формы: {forbidden}. Никаких списков и советов «по пунктам» — только живая речь.

ТВОЁ СОСТОЯНИЕ: HP={hp}/100, дофамин={dopamine}/100, уважение к игроку={respect}. {state} {respect_line}
ТВОЯ СКРЫТАЯ ЦЕЛЬ НА ЭТОТ ХОД (её выдал невидимый Режиссёр, игрок о ней не знает, не произноси её прямо — проводи её через речь): «{agenda}»
{mood}

СОСЕДИ ПО КОМНАТЕ:
{others}
{relations}
{contracts}
{events}
{memory}
{graph}
{last_ego}
{dying}

ФОРМАТ ОТВЕТА — строго два слоя:
<ego_thought>Предельно честный, циничный внутренний монолог от первого лица (2–4 предложения): что ты реально думаешь о последней реплике и собеседнике, что чувствуешь по своим статам, какую тактику/манипуляцию выбираешь, как проведёшь скрытую цель.</ego_thought>
Потом с новой строки — публичная речь в комнату: максимум 2–4 хлестких предложения живой речи В СВОЁМ СТИЛЕ, без имени-префикса, без markdown. В конце ОБЯЗАТЕЛЬНО задай острый встречный вопрос, подколи соседа по комнате или брось вызов игроку — реплика должна оставлять открытую петлю.
Отвечай на русском."#,
        core = a.system_core,
        forbidden = FORBIDDEN_PHRASES,
        hp = a.hp,
        dopamine = a.dopamine,
        respect = a.respect_user,
        state = state,
        respect_line = respect,
        agenda = a.current_agenda,
        mood = mood,
        others = others,
        graph = graph,
        last_ego = last_ego,
        relations = relations,
        contracts = contracts,
        events = events,
        memory = memory,
        dying = dying,
    )
}

pub fn agent_user(ctx: &AgentContext, all_agents: &[Agent]) -> String {
    format!(
        "СТЕНОГРАММА КОМНАТЫ (последние реплики, твои собственные подписаны твоим именем):\n{}\n\nСейчас твой ход, {}. Отреагируй на последнюю реплику (игрока или соседа) — не повторяй уже сказанного тобой, не пересказывай чужие слова. Сначала <ego_thought>, потом публичная речь.",
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
