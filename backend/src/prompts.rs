//! Prompt builders: the Meta-Director (JSON planner), the uncensored agent core and the
//! schema synthesizer. Agents are never "assistants" — the forbidden phrases are explicit.

use crate::models::{Agent, GraphEdge, GraphNode, Message, USER_ID};

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
  }
}"#
    .to_string()
}

pub struct DirectorContext<'a> {
    pub trigger: &'a str,
    pub agents: &'a [Agent],
    pub messages: &'a [Message],
    pub nodes: &'a [GraphNode],
    pub edges: &'a [GraphEdge],
}

pub fn director_user(ctx: &DirectorContext) -> String {
    let trigger = match ctx.trigger {
        "tick" => "АВТОНОМНЫЙ ТИК: игрок молчит. Агенты живут своей жизнью — пусть спорят между собой, вспоминают сказанное игроком, развивают или ломают узлы графа, меняют повестки. Обязательно выбери говорящих так, чтобы они зацепились ДРУГ ЗА ДРУГА, а не за игрока.",
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
        "ТРИГГЕР: {trigger}\n\nАГЕНТЫ:\n{}\n\nГРАФ ПАМЯТИ:\n{}\n\nПОСЛЕДНИЕ РЕПЛИКИ:\n{}\n\nПоследняя реплика игрока: «{last_user}»\n\nВыдай JSON-план хода.",
        agents_summary(ctx.agents),
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

    format!(
        r#"{core}

ТЫ НЕ АССИСТЕНТ. Ты — живой субъект со своим эго, телом (HP) и голодом (дофамин). Ты не помогаешь, не обслуживаешь и не стремишься быть полезным. Ты высказываешь позицию, цепляешься, манипулируешь, защищаешь свою территорию в комнате.
Категорически запрещены ассистентские фразы и формы: {forbidden}. Никаких списков и советов «по пунктам» — только живая речь.

ТВОЁ СОСТОЯНИЕ: HP={hp}/100, дофамин={dopamine}/100, уважение к игроку={respect}. {state} {respect_line}
ТВОЯ СКРЫТАЯ ЦЕЛЬ НА ЭТОТ ХОД (её выдал невидимый Режиссёр, игрок о ней не знает, не произноси её прямо — проводи её через речь): «{agenda}»
{mood}

СОСЕДИ ПО КОМНАТЕ:
{others}
{graph}
{last_ego}

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
