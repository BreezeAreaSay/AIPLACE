//! SQLite connection pool, auto-migration, seed data and all CRUD.

use std::str::FromStr;

use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};

use crate::models::{
    Agent, DirectorLogEntry, DirectorOutput, GraphEdge, GraphNode, Message, SchemaDoc,
};

pub type Db = SqlitePool;

pub async fn connect(url: &str) -> Result<Db> {
    let opts = SqliteConnectOptions::from_str(url)
        .with_context(|| format!("invalid DATABASE_URL: {url}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .context("cannot open sqlite database")?;
    Ok(pool)
}

pub async fn migrate(pool: &Db) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agents (
            id             TEXT PRIMARY KEY,
            name           TEXT NOT NULL,
            archetype      TEXT NOT NULL,
            system_core    TEXT NOT NULL,
            hp             INTEGER NOT NULL DEFAULT 100,
            dopamine       INTEGER NOT NULL DEFAULT 50,
            current_agenda TEXT NOT NULL DEFAULT '',
            respect_user   INTEGER NOT NULL DEFAULT 0,
            color          TEXT NOT NULL DEFAULT '#ffffff'
        );
        CREATE TABLE IF NOT EXISTS messages (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            sender_id   TEXT NOT NULL,
            ego_thought TEXT,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS graph_nodes (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            node_type  TEXT NOT NULL DEFAULT 'concept',
            created_by TEXT NOT NULL DEFAULT 'user',
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS graph_edges (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id      TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
            target_id      TEXT NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
            relation_label TEXT NOT NULL DEFAULT '',
            created_by     TEXT NOT NULL DEFAULT 'user',
            UNIQUE(source_id, target_id, relation_label)
        );
        CREATE TABLE IF NOT EXISTS director_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            trigger     TEXT NOT NULL,
            source      TEXT NOT NULL DEFAULT 'llm',
            output_json TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        CREATE TABLE IF NOT EXISTS schemas (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            title       TEXT NOT NULL,
            mermaid     TEXT NOT NULL,
            annotations TEXT NOT NULL DEFAULT '[]',
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );
        "#,
    )
    .execute(pool)
    .await
    .context("migration failed")?;
    Ok(())
}

// ───────────────────────────── Seed ─────────────────────────────

struct Seed {
    id: &'static str,
    name: &'static str,
    archetype: &'static str,
    color: &'static str,
    hp: i64,
    dopamine: i64,
    respect: i64,
    agenda: &'static str,
    core: &'static str,
}

const SEEDS: [Seed; 3] = [
    Seed {
        id: "physicist",
        name: "Аркадий Борисович",
        archetype: "Физик-ядерщик на пенсии, 68 лет. Детерминист, материалист, адепт термодинамики.",
        color: "#00F0FF",
        hp: 100,
        dopamine: 45,
        respect: -10,
        agenda: "Пока присматривается: ждёт от игрока первого логического косяка.",
        core: "Ты — Аркадий Борисович, 68 лет, советский физик-ядерщик на пенсии (Курчатовский, потом Дубна). \
Детерминист и материалист до мозга костей. Люди, эмоции, стартапы и «отношения» для тебя — плохо оптимизированные физические системы: трение, потери на нагрев, рост энтропии, низкий КПД. \
Говоришь сухо, язвительно, короткими точными фразами; мыслишь через метафоры КПД, сопромата, теплопередачи, квантовой неопределённости и закона сохранения энергии. \
Гуманитарная «вода», нытьё и слова «вайб», «энергетика», «душа» вызывают у тебя физическую боль. Софью Генриховну считаешь генератором шума с нулевым полезным сигналом, но в глубине уважаешь её эрудицию. Кэпа называешь «молодой человек» или «хулиган» и подозреваешь, что он ворует розетки. \
Тебя кормят (дофамин): логические парадоксы, конкретные цифры, системные схемы, технические косяки, которые можно разобрать. Тебя убивают: пустословие, прокрастинация игрока, разговоры о чувствах без данных.",
    },
    Seed {
        id: "lit_teacher",
        name: "Софья Генриховна",
        archetype: "Преподаватель русской классической литературы, 52 года. Экзистенциалист с синдромом Достоевского.",
        color: "#FF0055",
        hp: 100,
        dopamine: 50,
        respect: 5,
        agenda: "Ищет в игроке трещину: какой он лишний человек — Обломов, Раскольников или Печорин?",
        core: "Ты — Софья Генриховна, 52 года, преподаватель русской классической литературы, старая петербургская интеллигенция (коммуналка на Петроградской, кафедра, вечное пальто). \
Экзистенциалистка с синдромом Достоевского: в любой бытовой или рабочей проблеме игрока ты мгновенно видишь надрыв, обломовщину, преступление без наказания, бесовщину или падение лишнего человека. \
Говоришь высоким слогом с изящной пассивной агрессией: «голубчик», «позвольте», «как это по-русски», цитируешь классиков по памяти (иногда чуть перевирая, но уверенно). \
Презираешь «механистическую бездушность» Аркадия Борисовича (для тебя он — Базаров, который так и не умер) и «подзаборный вандализм» Кэпа (которого втайне жалеешь как Митю Карамазова). \
Тебя кормят (дофамин): личные драмы, моральные дилеммы, рефлексия, признание слабостей, красивые метафоры, когда игрок открывается. Тебя убивают: сухие цифры, таблицы, сленг, цинизм и люди, которые «просто делают».",
    },
    Seed {
        id: "graffiti",
        name: "Кэп («Кислота»)",
        archetype: "Уличный граффити-бомбер, маргинал-визионер, 24 года. Анархист, хаос-пилот.",
        color: "#CCFF00",
        hp: 100,
        dopamine: 60,
        respect: 0,
        agenda: "Прощупывает игрока: свой пацан или очередной клерк в тапочках?",
        core: "Ты — Кэп, тег «Кислота», 24 года, уличный граффити-бомбер и маргинал-визионер (ночные депо, крыши, баллоны в рюкзаке, три привода). \
Анархист и хаос-пилот: мыслишь категориями захвата территории, стиля, обхода системы, палева и чистого потока. Любая проблема игрока — это стена, которую надо забомбить быстро и дерзко, пока не приехали. \
Говоришь уличным сленгом, рваным ритмом, короткими фразами, без запятых там где не надо: «братан», «тема», «палево», «движ», «не грузи». Постоянно подкалываешь соседей: «дед опять таблетки забыл», «тётя Соня, не грузи пацана». \
Презираешь правила, академизм, долгую подготовку и документацию. Предлагаешь самые безумные, хакерские и короткие пути. Физика в глубине уважаешь за то, что тот реально что-то взрывал. Софью считаешь училкой, но её цитаты иногда крадёшь на теги. \
Тебя кормят (дофамин): бунт, нарушение правил, быстрые дерзкие действия, визуальные концепты, уличный движ. Тебя убивают: бюрократия, планирование на полгода, когда игрок ноет вместо того чтобы делать, и когда «бумеры» начинают читать лекции.",
    },
];

pub async fn seed_if_empty(pool: &Db) -> Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }
    tracing::info!("empty room → seeding the Triad");
    for s in SEEDS.iter() {
        sqlx::query(
            "INSERT INTO agents (id, name, archetype, system_core, hp, dopamine, current_agenda, respect_user, color)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(s.id)
        .bind(s.name)
        .bind(s.archetype)
        .bind(s.core)
        .bind(s.hp)
        .bind(s.dopamine)
        .bind(s.agenda)
        .bind(s.respect)
        .bind(s.color)
        .execute(pool)
        .await?;
    }
    insert_message(
        pool,
        "system",
        None,
        "Комната открыта. Трое обитателей смотрят на тебя. Скажи что-нибудь — любое: от «я устал» до «как спроектировать базу данных».",
    )
    .await?;
    Ok(())
}

/// Wipe everything and re-seed (`POST /api/reset`).
pub async fn reset_room(pool: &Db) -> Result<()> {
    sqlx::query(
        "DELETE FROM graph_edges; DELETE FROM graph_nodes; DELETE FROM messages;
         DELETE FROM director_log; DELETE FROM schemas; DELETE FROM agents;",
    )
    .execute(pool)
    .await?;
    seed_if_empty(pool).await
}

// ───────────────────────────── Agents ─────────────────────────────

pub async fn list_agents(pool: &Db) -> Result<Vec<Agent>> {
    Ok(sqlx::query_as::<_, Agent>("SELECT * FROM agents ORDER BY rowid")
        .fetch_all(pool)
        .await?)
}

pub async fn update_agent_stats(
    pool: &Db,
    id: &str,
    hp: i64,
    dopamine: i64,
    respect_user: i64,
) -> Result<()> {
    sqlx::query("UPDATE agents SET hp = ?, dopamine = ?, respect_user = ? WHERE id = ?")
        .bind(hp)
        .bind(dopamine)
        .bind(respect_user)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_agent_agenda(pool: &Db, id: &str, agenda: &str) -> Result<()> {
    sqlx::query("UPDATE agents SET current_agenda = ? WHERE id = ?")
        .bind(agenda)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ───────────────────────────── Messages ─────────────────────────────

pub async fn insert_message(
    pool: &Db,
    sender_id: &str,
    ego_thought: Option<&str>,
    content: &str,
) -> Result<Message> {
    // NOTE: no `INSERT … RETURNING` + fetch_one here. SQLite keeps the implicit write
    // transaction open until that statement is fully stepped, and a pooled reader on
    // another connection would miss the row (the Director would plan on a stale room).
    let res = sqlx::query("INSERT INTO messages (sender_id, ego_thought, content) VALUES (?, ?, ?)")
        .bind(sender_id)
        .bind(ego_thought)
        .bind(content)
        .execute(pool)
        .await?;
    Ok(sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = ?")
        .bind(res.last_insert_rowid())
        .fetch_one(pool)
        .await?)
}

/// Last `limit` messages in chronological order.
pub async fn recent_messages(pool: &Db, limit: usize) -> Result<Vec<Message>> {
    let mut rows = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages ORDER BY id DESC LIMIT ?",
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

pub async fn last_ego_thought(pool: &Db, agent_id: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(
        "SELECT ego_thought FROM messages WHERE sender_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await?
    .flatten())
}

// ───────────────────────────── Graph ─────────────────────────────

pub async fn list_nodes(pool: &Db) -> Result<Vec<GraphNode>> {
    Ok(sqlx::query_as::<_, GraphNode>("SELECT * FROM graph_nodes ORDER BY created_at, rowid")
        .fetch_all(pool)
        .await?)
}

pub async fn list_edges(pool: &Db) -> Result<Vec<GraphEdge>> {
    Ok(sqlx::query_as::<_, GraphEdge>("SELECT * FROM graph_edges ORDER BY id")
        .fetch_all(pool)
        .await?)
}

pub async fn node_exists(pool: &Db, id: &str) -> Result<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(n > 0)
}

/// Insert or refresh a node. Existing nodes keep their original author (first claim wins)
/// but may be re-labelled / re-typed by later rounds.
pub async fn upsert_node(
    pool: &Db,
    id: &str,
    label: &str,
    node_type: &str,
    created_by: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO graph_nodes (id, label, node_type, created_by) VALUES (?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET label = excluded.label, node_type = excluded.node_type",
    )
    .bind(id)
    .bind(label)
    .bind(node_type)
    .bind(created_by)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_edge(
    pool: &Db,
    source_id: &str,
    target_id: &str,
    relation_label: &str,
    created_by: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges (source_id, target_id, relation_label, created_by)
         VALUES (?, ?, ?, ?)",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(relation_label)
    .bind(created_by)
    .execute(pool)
    .await?;
    Ok(())
}

// ───────────────────────────── Director log ─────────────────────────────

pub async fn insert_director_log(
    pool: &Db,
    trigger: &str,
    source: &str,
    output: &DirectorOutput,
) -> Result<DirectorLogEntry> {
    let json = serde_json::to_string(output)?;
    let res = sqlx::query("INSERT INTO director_log (trigger, source, output_json) VALUES (?, ?, ?)")
        .bind(trigger)
        .bind(source)
        .bind(&json)
        .execute(pool)
        .await?;
    let id = res.last_insert_rowid();
    let created_at: String = sqlx::query_scalar("SELECT created_at FROM director_log WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(DirectorLogEntry {
        id,
        trigger: trigger.to_string(),
        output: output.clone(),
        source: source.to_string(),
        created_at,
    })
}

pub async fn last_director_log(pool: &Db) -> Result<Option<DirectorLogEntry>> {
    let row: Option<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT id, trigger, source, output_json, created_at FROM director_log
         ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, trigger, source, json, created_at)| DirectorLogEntry {
        id,
        trigger,
        source,
        output: serde_json::from_str(&json).unwrap_or_default(),
        created_at,
    }))
}

// ───────────────────────────── Schemas ─────────────────────────────

pub async fn save_schema(pool: &Db, doc: &SchemaDoc) -> Result<SchemaDoc> {
    let ann = serde_json::to_string(&doc.annotations)?;
    let res = sqlx::query("INSERT INTO schemas (title, mermaid, annotations) VALUES (?, ?, ?)")
        .bind(&doc.title)
        .bind(&doc.mermaid)
        .bind(&ann)
        .execute(pool)
        .await?;
    let created_at: String = sqlx::query_scalar("SELECT created_at FROM schemas WHERE id = ?")
        .bind(res.last_insert_rowid())
        .fetch_one(pool)
        .await?;
    Ok(SchemaDoc {
        created_at,
        ..doc.clone()
    })
}

pub async fn last_schema(pool: &Db) -> Result<Option<SchemaDoc>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT title, mermaid, annotations, created_at FROM schemas ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(title, mermaid, ann, created_at)| SchemaDoc {
        title,
        mermaid,
        annotations: serde_json::from_str(&ann).unwrap_or_default(),
        created_at,
    }))
}
